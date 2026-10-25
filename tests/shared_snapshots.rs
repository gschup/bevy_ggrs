//! A component type nothing changed since the last save shares that save's
//! snapshot instead of copying every component, and a load of a frame that
//! shares the snapshot the world holds writes nothing ([`HeldComponentSnapshot`]).
//!
//! The SyncTest sessions roll back every frame and compare checksums, so a
//! share that missed a change, an add or a removal fails them with a
//! `SyncTestMismatch`.

#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

/// Never changes after spawn.
#[derive(Component, Clone, Copy, Hash, Debug, PartialEq)]
struct Still(u32);

/// Changes every fifth frame.
#[derive(Component, Clone, Copy, Hash, Debug, PartialEq)]
struct Sometimes(u32);

/// Moves from one entity to another at frame 12, so the type keeps its count.
#[derive(Component, Clone, Copy, Hash, Debug, PartialEq)]
struct Swapped(u32);

#[derive(Component)]
struct First;

#[derive(Component)]
struct Second;

fn spawn(mut commands: Commands) {
    commands.spawn((Still(7), Sometimes(0), Swapped(1), First, Rollback));
    commands.spawn((Still(8), Second, Rollback));
}

fn advance(
    mut commands: Commands,
    frame: Res<RollbackFrameCount>,
    mut sometimes: Query<&mut Sometimes>,
    first: Query<Entity, With<First>>,
    second: Query<Entity, With<Second>>,
) {
    if frame.0 % 5 == 0 {
        for mut value in &mut sometimes {
            value.0 += 1;
        }
    }
    if frame.0 == 12 {
        commands.entity(first.single().unwrap()).remove::<Swapped>();
        commands.entity(second.single().unwrap()).insert(Swapped(2));
    }
}

fn app() -> App {
    let mut app = base_synctest_app(3);
    app.add_systems(Startup, spawn)
        .rollback_component_with_copy::<Still>()
        .rollback_component_with_copy::<Sometimes>()
        .rollback_component_with_copy::<Swapped>()
        .checksum_component_with_hash::<Still>()
        .checksum_component_with_hash::<Sometimes>()
        .checksum_component_with_hash::<Swapped>()
        .add_systems(GgrsSchedule, advance);
    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: a shared snapshot parted from the world");
    });
    app
}

/// The newest saved snapshot of `T`.
fn newest<T: Component>(app: &App) -> GgrsComponentSnapshot<T> {
    let frame = app
        .world()
        .resource::<SnapshotFrames>()
        .saved()
        .last()
        .unwrap();
    app.world()
        .resource::<GgrsComponentSnapshots<T>>()
        .peek(frame)
        .unwrap()
        .clone()
}

#[test]
fn an_unchanged_type_shares_one_snapshot_through_rollbacks() {
    let mut app = app();
    // The first update runs Startup; the first save is the second's.
    app.update();
    app.update();
    let (first_still, mut still_copies, mut sometimes_copies) = (newest::<Still>(&app), 0, 0);
    let mut last = (first_still.clone(), newest::<Sometimes>(&app));
    for _ in 0..30 {
        app.update();
        let now = (newest::<Still>(&app), newest::<Sometimes>(&app));
        still_copies += !now.0.same(&last.0) as u32;
        sometimes_copies += !now.1.same(&last.1) as u32;
        last = now;
    }
    assert_eq!(still_copies, 0, "Still never changes");
    assert!(last.0.same(&first_still));
    assert_eq!(last.0.len(), 2);
    assert!(
        (5..30).contains(&sometimes_copies),
        "Sometimes changes every fifth frame, and is shared between: {sometimes_copies} copies"
    );
}

#[test]
fn a_change_an_add_and_a_removal_are_each_saved() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    let world = app.world_mut();
    let frame = world.resource::<RollbackFrameCount>().0 as u32;
    let sometimes = world.query::<&Sometimes>().single(world).unwrap().0;
    let advanced_on_a_fifth: u32 = (1..=frame).filter(|f| f % 5 == 0).count() as u32;
    assert_eq!(
        sometimes, advanced_on_a_fifth,
        "Sometimes counts every fifth frame"
    );
    let swapped: Vec<(bool, u32)> = world
        .query::<(Has<Second>, &Swapped)>()
        .iter(world)
        .map(|(second, value)| (second, value.0))
        .collect();
    assert_eq!(
        swapped,
        vec![(true, 2)],
        "Swapped moved to the second entity"
    );
}

/// A load that shares the held snapshot writes nothing, so it leaves the
/// component unchanged in Bevy's change detection.
#[test]
fn a_load_onto_the_held_snapshot_marks_nothing_changed() {
    #[derive(Resource, Default)]
    struct StillChanges(u32);

    let mut app = app();
    app.init_resource::<StillChanges>().add_systems(
        Update,
        |still: Query<Ref<Still>>, mut changes: ResMut<StillChanges>| {
            changes.0 += still.iter().filter(|s| s.is_changed()).count() as u32;
        },
    );
    for _ in 0..30 {
        app.update();
    }
    // Two at spawn, seen once; none from the rollbacks after.
    assert_eq!(app.world().resource::<StillChanges>().0, 2);
}

/// One of many entities, picked by the frame.
#[derive(Component, Clone, Copy, Hash, Debug, PartialEq)]
struct Many(u32);

/// Where an entity of `Many` sits, to pick it by.
#[derive(Component)]
struct Slot(u32);

const MANY: u32 = 200;

fn many_app() -> App {
    let mut app = base_synctest_app(3);
    app.add_systems(Startup, |mut commands: Commands| {
        for slot in 0..MANY {
            commands.spawn((Many(0), Slot(slot), Rollback));
        }
    })
    .rollback_component_with_copy::<Many>()
    .checksum_component_with_hash::<Many>()
    .add_systems(
        GgrsSchedule,
        |mut commands: Commands,
         frame: Res<RollbackFrameCount>,
         mut many: Query<(Entity, &Slot, &mut Many)>| {
            let picked = frame.0 as u32 % MANY;
            for (entity, slot, mut value) in &mut many {
                if slot.0 == picked {
                    value.0 += 1;
                }
                // Every seventh frame one entity goes and another comes, so
                // the entity set and the type's count change too.
                if frame.0 % 7 == 0 && slot.0 == (picked + 100) % MANY {
                    commands.entity(entity).despawn_rollback();
                    commands.spawn((Many(frame.0 as u32), Slot(slot.0), Rollback));
                }
                // Every eleventh frame one entity loses the type, a removal
                // that leaves no change tick behind.
                if frame.0 % 11 == 0 && slot.0 == (picked + 50) % MANY {
                    commands.entity(entity).remove::<Many>();
                }
            }
        },
    );
    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: a shared chunk parted from the world");
    });
    app
}

/// The newest saved snapshot of `T` and the one before it.
fn newest_two<T: Send + Sync + 'static>(
    app: &App,
) -> (GgrsComponentSnapshot<T>, GgrsComponentSnapshot<T>) {
    let frames: Vec<i32> = app.world().resource::<SnapshotFrames>().saved().collect();
    let store = app.world().resource::<GgrsComponentSnapshots<T>>();
    let [.., before, last] = frames[..] else {
        panic!("two frames saved")
    };
    (
        store.peek(last).unwrap().clone(),
        store.peek(before).unwrap().clone(),
    )
}

/// A frame that changes one entity of many copies that entity's chunk and
/// shares every other with the frame before, through the SyncTest's
/// rollbacks; spawns, despawns and removals copy their chunks and match the
/// checksums.
#[test]
fn a_change_to_one_entity_copies_only_its_chunk() {
    let mut app = many_app();
    app.update();
    app.update();
    let mut checked = 0;
    for _ in 0..60 {
        app.update();
        let frame = app.world().resource::<RollbackFrameCount>().0;
        let (last, before) = newest_two::<Many>(&app);
        let (shared, chunks) = last.chunks_shared_with(&before);
        // A frame that spawned, despawned or removed copies up to four chunks.
        let copied = chunks - shared;
        assert!(
            (1..=4).contains(&copied),
            "frame {frame}: {copied} of {chunks} chunks copied"
        );
        checked += (copied == 1) as u32;

        let (last, before) = newest_two::<Entity>(&app);
        let (shared, chunks) = last.chunks_shared_with(&before);
        assert!(
            chunks - shared <= 3,
            "frame {frame}: entities copied {}",
            chunks - shared
        );
    }
    assert!(checked > 40, "most frames copy one chunk: {checked}");
    let world = app.world_mut();
    let frame = world.resource::<RollbackFrameCount>().0;
    let removed = (1..frame).filter(|f| f % 11 == 0).count();
    let count = world.query::<&Many>().iter(world).count();
    assert_eq!(
        count,
        MANY as usize - removed,
        "one removed every eleventh frame"
    );
}

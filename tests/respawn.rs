//! Tests for the components a rollback inserts on a respawned entity.
//!
//! A respawned entity gets every snapshot component in one insert (`insert_staged_components`),
//! so `#[require]` never fills a placeholder for a component whose snapshot value is on its
//! way, and every hook sees all the snapshot values. Each test despawns its entity outright at
//! frame 3 in a SyncTest with `check_distance = 4`, so every rollback across that frame
//! respawns it. `despawn_rollback` would not do: a rollback lifts its mark with the components
//! kept, and nothing is inserted.

#[allow(dead_code)]
mod common;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

/// Despawns every entity with `T` outright at frame 3.
fn despawn_at_frame_3<T: Component>(
    mut commands: Commands,
    frame: Res<RollbackFrameCount>,
    query: Query<Entity, With<T>>,
) {
    if frame.0 == 3 {
        for entity in &query {
            commands.entity(entity).despawn();
        }
    }
}

/// Runs the session past the frames whose rollbacks cross frame 3. The entity is gone at
/// the end, despawned at frame 3 by every resimulation, so what a respawn inserted is
/// checked from inside the session, by a `GgrsSchedule` system.
fn run_past_the_respawns(app: &mut App) {
    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: the respawn is non-deterministic");
    });
    for _ in 0..20 {
        app.update();
    }
    assert!(app.world().resource::<RollbackFrameCount>().0 > 7);
}

/// `Body` requires `Index`, whose default is unassigned and whose insert hook then assigns the
/// next number from `Issued`, the way an id allocator does. `Body` is registered first, so
/// inserted one type at a time its default `Index` would land before the snapshot `Index`, and
/// the hook would run on the placeholder. In one insert the snapshot `Index` is in the bundle
/// and `#[require]` has nothing to fill.
#[test]
fn a_respawn_never_inserts_a_required_default() {
    static PLACEHOLDERS: AtomicU32 = AtomicU32::new(0);
    const UNASSIGNED: u32 = u32::MAX;

    #[derive(Component, Copy, Clone)]
    #[require(Index)]
    struct Body;

    #[derive(Component, Copy, Clone, Hash, PartialEq, Eq, Debug)]
    #[component(on_insert = assign_index)]
    struct Index(u32);

    impl Default for Index {
        fn default() -> Self {
            Self(UNASSIGNED)
        }
    }

    #[derive(Resource, Copy, Clone, Hash, Default)]
    struct Issued(u32);

    fn assign_index(mut world: DeferredWorld, context: HookContext) {
        if world.get::<Index>(context.entity).unwrap().0 != UNASSIGNED {
            return;
        }
        PLACEHOLDERS.fetch_add(1, Ordering::SeqCst);
        let next = world.resource::<Issued>().0;
        world.get_mut::<Index>(context.entity).unwrap().0 = next;
        world.resource_mut::<Issued>().0 = next + 1;
    }

    fn keeps_its_index(bodies: Query<&Index, With<Body>>) {
        for index in &bodies {
            assert_eq!(
                *index,
                Index(0),
                "the respawned Body kept the index it was spawned with"
            );
        }
    }

    let mut app = base_synctest_app(4);
    app.init_resource::<Issued>()
        .rollback_component_with_copy::<Body>()
        .rollback_component_with_copy::<Index>()
        .rollback_resource_with_copy::<Issued>()
        .checksum_component_with_hash::<Index>()
        .checksum_resource_with_hash::<Issued>()
        .add_systems(
            GgrsSchedule,
            (keeps_its_index, despawn_at_frame_3::<Body>).chain(),
        );
    app.world_mut().spawn((Body, Rollback));

    run_past_the_respawns(&mut app);

    assert_eq!(
        PLACEHOLDERS.load(Ordering::SeqCst),
        1,
        "the hook saw an unassigned Index once, at the spawn, and never on a respawn"
    );
}

/// `Left` and `Right` have no required-component relation, and each one's insert hook looks
/// for the other on the entity. In one insert both values are written before either hook
/// runs; one type at a time, the first hook would find the other component missing.
#[test]
fn a_respawn_inserts_every_snapshot_component_at_once() {
    static MISSES: AtomicU32 = AtomicU32::new(0);

    #[derive(Component, Copy, Clone)]
    #[component(on_insert = expect_right)]
    struct Left;

    #[derive(Component, Copy, Clone)]
    #[component(on_insert = expect_left)]
    struct Right;

    fn expect_right(world: DeferredWorld, context: HookContext) {
        if world.get::<Right>(context.entity).is_none() {
            MISSES.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn expect_left(world: DeferredWorld, context: HookContext) {
        if world.get::<Left>(context.entity).is_none() {
            MISSES.fetch_add(1, Ordering::SeqCst);
        }
    }

    let mut app = base_synctest_app(4);
    app.rollback_component_with_copy::<Left>()
        .rollback_component_with_copy::<Right>()
        .add_systems(GgrsSchedule, despawn_at_frame_3::<Left>);
    app.world_mut().spawn((Left, Right, Rollback));

    run_past_the_respawns(&mut app);

    assert_eq!(
        MISSES.load(Ordering::SeqCst),
        0,
        "a hook ran before the other snapshot component was written"
    );
}

/// A value a respawn inserts is the world's from then on: dropped once, when the entity or
/// the world goes, and never twice. `Counted` owns heap memory, so a double drop would be a
/// double free, and `LIVE` counts constructions against drops across the whole session,
/// snapshot clones included.
#[test]
fn a_respawned_value_is_dropped_exactly_once() {
    static LIVE: AtomicIsize = AtomicIsize::new(0);

    #[derive(Component)]
    struct Counted(Box<u32>);

    impl Counted {
        fn new(value: u32) -> Self {
            LIVE.fetch_add(1, Ordering::SeqCst);
            Self(Box::new(value))
        }
    }

    impl Clone for Counted {
        fn clone(&self) -> Self {
            Self::new(*self.0)
        }
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            LIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn reads_as_the_snapshot(counted: Query<&Counted>) {
        for counted in &counted {
            assert_eq!(*counted.0, 7, "the respawned value is the snapshot's");
        }
    }

    let mut app = base_synctest_app(4);
    app.rollback_component_with_clone::<Counted>().add_systems(
        GgrsSchedule,
        (reads_as_the_snapshot, despawn_at_frame_3::<Counted>).chain(),
    );
    app.world_mut().spawn((Counted::new(7), Rollback));

    run_past_the_respawns(&mut app);
    drop(app);
    assert_eq!(
        LIVE.load(Ordering::SeqCst),
        0,
        "every Counted constructed was dropped once"
    );
}

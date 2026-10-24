//! A component type's checksum rehashes only the components changed since the
//! last one ([`ComponentChecksumPlugin`]). Whatever changes, adds and removals
//! came before, it must equal the checksum of a world built straight into the
//! same state, where every component is hashed fresh.

#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

#[derive(Component, Clone, Copy, Hash, Debug, PartialEq)]
struct Value(u32);

/// Which of the three entities this is, so systems can find it.
#[derive(Component, Clone, Copy, PartialEq)]
struct Slot(u8);

/// Changes slot 0's value at frame 8 for good, slot 1's at frame 10 and back
/// at frame 20, and takes slot 2's away at frame 15: a change that stays, a
/// value restored, and a removal.
fn history(
    mut commands: Commands,
    frame: Res<RollbackFrameCount>,
    mut values: Query<(Entity, &Slot, &mut Value)>,
) {
    for (entity, slot, mut value) in &mut values {
        match (frame.0, slot.0) {
            (8, 0) => value.0 = 5,
            (10, 1) => value.0 = 20,
            (20, 1) => value.0 = 2,
            (15, 2) => {
                commands.entity(entity).remove::<Value>();
            }
            _ => {}
        }
    }
}

fn app(spawn: fn(Commands), history_on: bool) -> App {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, spawn)
        .rollback_component_with_copy::<Value>()
        .checksum_component_with_hash::<Value>();
    if history_on {
        app.add_systems(GgrsSchedule, history);
    }
    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: the checksum parted from the world");
    });
    app
}

fn checksum(app: &mut App) -> u128 {
    let world = app.world_mut();
    world
        .query_filtered::<&ChecksumPart, With<ChecksumFlag<Value>>>()
        .single(world)
        .unwrap()
        .0
}

fn three(mut commands: Commands) {
    commands.spawn((Value(0), Slot(0), Rollback));
    commands.spawn((Value(2), Slot(1), Rollback));
    commands.spawn((Value(3), Slot(2), Rollback));
}

/// Where `three` ends up after `history`: slot 0 at 5, slot 2 without its
/// value.
fn three_after(mut commands: Commands) {
    commands.spawn((Value(5), Slot(0), Rollback));
    commands.spawn((Value(2), Slot(1), Rollback));
    commands.spawn((Slot(2), Rollback));
}

#[test]
fn a_checksum_after_changes_equals_one_taken_fresh() {
    let mut lived = app(three, true);
    for _ in 0..12 {
        lived.update();
    }
    let changed = checksum(&mut lived);
    for _ in 12..30 {
        lived.update();
    }
    let mut fresh = app(three_after, false);
    for _ in 0..30 {
        fresh.update();
    }
    assert_eq!(checksum(&mut lived), checksum(&mut fresh));
    assert_ne!(
        changed,
        checksum(&mut fresh),
        "slot 1 held 20 and slot 2 its value at frame 12"
    );
}

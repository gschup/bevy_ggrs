//! Tests for component snapshot strategies: Copy, Clone, and Reflect — for both mutable and
//! immutable component types.
//!
//! Each test runs a SyncTest session that increments a component value each frame via game logic,
//! then verifies the final value equals the rollback frame count. A `SyncTestMismatch` observer
//! panics immediately if rollback produces a different checksum during re-simulation, catching
//! any correctness regression in the snapshot strategies.

#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

// ---- Mutable component — Reflect strategy ----

/// A data-carrying component registered via [`rollback_component_with_reflect`].
///
/// Unlike the empty marker structs in `hierarchy.rs`, this has a field value that must
/// be correctly saved and restored to exercise the Reflect `apply` path.
#[derive(Component, Reflect, Default, Clone, Hash, Debug, PartialEq)]
struct Score(u32);

fn increment_score(mut query: Query<&mut Score, With<Rollback>>) {
    for mut score in &mut query {
        score.0 += 1;
    }
}

/// Verifies that a data-carrying mutable component registered with `rollback_component_with_reflect`
/// is correctly saved and restored across rollbacks via the Reflect `apply` path.
///
/// `hierarchy.rs` already covers Reflect rollback for empty marker structs. This test
/// complements it by verifying actual field data is preserved.
#[test]
fn reflect_strategy_rolls_back_component_data() {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((Score(0), Rollback));
    })
    .rollback_component_with_reflect::<Score>()
    .checksum_component_with_hash::<Score>()
    .add_systems(GgrsSchedule, increment_score);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: Reflect strategy component rollback is non-deterministic");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0 as u32;
    let score = app
        .world_mut()
        .query::<&Score>()
        .single(app.world())
        .unwrap()
        .0;
    assert_eq!(
        score, frame,
        "Score (Reflect) should equal the rollback frame count after {updates} updates"
    );
}

// ---- Immutable components ----
//
// `#[component(immutable)]` components cannot be mutated in-place; game logic must
// remove and re-insert them. The rollback restore path also re-inserts (triggering hooks)
// rather than writing through `&mut T`. These tests verify all three snapshot strategies
// work correctly for immutable components.

// --- Copy strategy ---

#[derive(Component, Copy, Clone, Hash, Default, Debug, PartialEq)]
#[component(immutable)]
struct ImmutableTick(u32);

fn increment_tick(mut commands: Commands, query: Query<(Entity, &ImmutableTick), With<Rollback>>) {
    for (entity, tick) in &query {
        commands.entity(entity).insert(ImmutableTick(tick.0 + 1));
    }
}

/// Verifies that an `#[component(immutable)]` type using the Copy snapshot strategy is
/// correctly saved and restored across rollbacks. The SyncTest session re-simulates every
/// `check_distance` frames; a `SyncTestMismatch` would indicate broken rollback.
#[test]
fn immutable_component_copy_strategy_rolls_back() {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((ImmutableTick(0), Rollback));
    })
    .rollback_immutable_component_with_copy::<ImmutableTick>()
    .checksum_component_with_hash::<ImmutableTick>()
    .add_systems(GgrsSchedule, increment_tick);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: immutable Copy component rollback is non-deterministic");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0 as u32;
    let tick = app
        .world_mut()
        .query::<&ImmutableTick>()
        .single(app.world())
        .unwrap()
        .0;
    assert_eq!(
        tick, frame,
        "ImmutableTick (Copy) should equal the rollback frame count after {updates} updates"
    );
}

// --- Clone strategy ---

// Clone but not Copy, to exercise the Clone code path.
#[derive(Component, Clone, Hash, Debug, PartialEq)]
#[component(immutable)]
struct ImmutableLabel(String);

impl Default for ImmutableLabel {
    fn default() -> Self {
        Self(String::from("frame_0"))
    }
}

fn update_label(
    mut commands: Commands,
    frame: Res<RollbackFrameCount>,
    query: Query<Entity, (With<ImmutableLabel>, With<Rollback>)>,
) {
    for entity in &query {
        commands
            .entity(entity)
            .insert(ImmutableLabel(format!("frame_{}", frame.0)));
    }
}

/// Verifies that an `#[component(immutable)]` type using the Clone snapshot strategy is
/// correctly saved and restored across rollbacks.
#[test]
fn immutable_component_clone_strategy_rolls_back() {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((ImmutableLabel::default(), Rollback));
    })
    .rollback_immutable_component_with_clone::<ImmutableLabel>()
    .checksum_component_with_hash::<ImmutableLabel>()
    .add_systems(GgrsSchedule, update_label);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: immutable Clone component rollback is non-deterministic");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0;
    let label = app
        .world_mut()
        .query::<&ImmutableLabel>()
        .single(app.world())
        .unwrap()
        .0
        .clone();
    assert_eq!(
        label,
        format!("frame_{frame}"),
        "ImmutableLabel (Clone) should reflect the current rollback frame after {updates} updates"
    );
}

// --- Reflect strategy ---

#[derive(Component, Reflect, Clone, Hash, Default, Debug, PartialEq)]
#[component(immutable)]
struct ImmutableGeneration(u32);

fn increment_generation(
    mut commands: Commands,
    query: Query<(Entity, &ImmutableGeneration), With<Rollback>>,
) {
    for (entity, generation) in &query {
        commands
            .entity(entity)
            .insert(ImmutableGeneration(generation.0 + 1));
    }
}

/// Verifies that an `#[component(immutable)]` type using the Reflect snapshot strategy is
/// correctly saved and restored across rollbacks.
#[test]
fn immutable_component_reflect_strategy_rolls_back() {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((ImmutableGeneration(0), Rollback));
    })
    .rollback_immutable_component_with_reflect::<ImmutableGeneration>()
    .checksum_component_with_hash::<ImmutableGeneration>()
    .add_systems(GgrsSchedule, increment_generation);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: immutable Reflect component rollback is non-deterministic");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0 as u32;
    let generation = app
        .world_mut()
        .query::<&ImmutableGeneration>()
        .single(app.world())
        .unwrap()
        .0;
    assert_eq!(
        generation, frame,
        "ImmutableGeneration (Reflect) should equal the rollback frame count after {updates} updates"
    );
}

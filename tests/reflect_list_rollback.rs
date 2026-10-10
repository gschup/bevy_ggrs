//! Tests that the Reflect snapshot strategy restores lists exactly, for both components and
//! resources.
//!
//! `PartialReflect::apply` never removes list elements beyond the source's length, so a naive
//! in-place restore keeps elements pushed after the snapshot was taken. Each test pushes to a
//! `Vec` every frame, so every SyncTest rollback restores a shorter list than the live one. A
//! stale tail changes the checksum during re-simulation, and the `SyncTestMismatch` observer
//! panics.

#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

/// A component holding a list that grows by one element per frame.
#[derive(Component, Reflect, Default, Clone, Hash, Debug, PartialEq)]
struct Trail(Vec<i32>);

fn extend_trail(frame: Res<RollbackFrameCount>, mut query: Query<&mut Trail, With<Rollback>>) {
    for mut trail in &mut query {
        trail.0.push(frame.0);
    }
}

/// Verifies that a `Vec` inside a component registered with `rollback_component_with_reflect`
/// is truncated back to its snapshot length on rollback.
#[test]
fn reflect_strategy_rolls_back_component_list() {
    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn((Trail::default(), Rollback));
    })
    .rollback_component_with_reflect::<Trail>()
    .checksum_component_with_hash::<Trail>()
    .add_systems(GgrsSchedule, extend_trail);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: Reflect strategy kept list elements across a rollback");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0;
    let trail = app
        .world_mut()
        .query::<&Trail>()
        .single(app.world())
        .unwrap()
        .0
        .clone();
    assert_eq!(
        trail,
        (1..=frame).collect::<Vec<_>>(),
        "Trail should hold exactly one entry per simulated frame after {updates} updates"
    );
}

/// A resource holding a list that grows by one element per frame.
#[derive(Resource, Reflect, Default, Clone, Hash, Debug, PartialEq)]
struct History(Vec<i32>);

fn extend_history(frame: Res<RollbackFrameCount>, mut history: ResMut<History>) {
    history.0.push(frame.0);
}

/// Verifies that a `Vec` inside a resource registered with `rollback_resource_with_reflect`
/// is truncated back to its snapshot length on rollback.
#[test]
fn reflect_strategy_rolls_back_resource_list() {
    let mut app = base_synctest_app(2);
    app.init_resource::<History>()
        .rollback_resource_with_reflect::<History>()
        .checksum_resource_with_hash::<History>()
        .add_systems(GgrsSchedule, extend_history);

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: Reflect strategy kept list elements across a rollback");
    });

    let updates = 20usize;
    for _ in 0..updates {
        app.update();
    }

    let frame = app.world().resource::<RollbackFrameCount>().0;
    assert_eq!(
        app.world().resource::<History>().0,
        (1..=frame).collect::<Vec<_>>(),
        "History should hold exactly one entry per simulated frame after {updates} updates"
    );
}

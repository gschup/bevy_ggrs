#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;
use std::sync::atomic::{AtomicU32, Ordering};

// --- Helpers specific to this file ---

#[derive(Reflect, Resource, Default, Debug, Clone)]
struct FrameCounter(u16);

fn frame_counter(mut counter: ResMut<FrameCounter>) {
    counter.0 = counter.0.wrapping_add(1);
}

fn create_app(check_distance: usize) -> App {
    let mut app = base_synctest_app(check_distance);
    app.init_resource::<FrameCounter>()
        .rollback_resource_with_clone::<FrameCounter>()
        .add_systems(GgrsSchedule, frame_counter);
    app
}

#[derive(Component, Hash, Debug, Clone, Copy)]
struct Health(u32);

impl Default for Health {
    fn default() -> Self {
        Self(10)
    }
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((Health::default(), Rollback));
}

fn decrease_health(mut commands: Commands, mut players: Query<(Entity, &mut Health)>) {
    for (entity, mut health) in &mut players {
        health.0 = health.0.saturating_sub(1);
        if health.0 == 0 {
            commands.entity(entity).despawn();
        }
    }
}

fn respawn_entity_app() -> App {
    let mut app = base_synctest_app(5);
    app.rollback_component_with_copy::<Health>()
        .add_systems(Startup, spawn_player)
        .add_systems(GgrsSchedule, decrease_health);
    app
}

// --- Tests ---

/// Verifies that despawning a rollback entity and rolling back across the despawn does not panic,
/// and that the entity is eventually confirmed as gone.
#[test]
fn despawn_and_rollback_does_not_panic() {
    let mut app = respawn_entity_app();

    // Health starts at 10, decrements by 1 per frame — despawn happens at frame 10.
    // Run well beyond that to ensure the despawn is confirmed across rollbacks.
    for _ in 0..60 {
        app.update();
    }

    // If we reach here without panicking, the rollback across despawn succeeded.
    // Also assert the entity is actually gone.
    assert!(
        app.world_mut().query::<&Health>().iter(app.world()).count() == 0,
        "Player entity should be despawned and confirmed gone after 60 updates"
    );
}

/// Verifies that `SyncTestMismatch` is triggered when game logic is non-deterministic.
///
/// A global atomic counter (not part of ECS and never rolled back) is incremented each time
/// the game system runs. The system writes this counter's value into a checksummed component,
/// so each re-simulation produces a different value — causing a checksum mismatch and firing
/// `SyncTestMismatch`. This confirms that the desync-detection pipeline works end-to-end.
#[test]
fn synctest_mismatch_fires_on_non_determinism() {
    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    #[derive(Component, Hash, Clone, Copy, Default)]
    struct Counter(u32);

    fn non_deterministic_counter(mut query: Query<&mut Counter, With<Rollback>>) {
        let count = CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        for mut c in &mut query {
            c.0 = count;
        }
    }

    fn spawn_counter(mut commands: Commands) {
        commands.spawn((Counter::default(), Rollback));
    }

    #[derive(Resource, Default)]
    struct MismatchDetected(bool);

    let mut app = base_synctest_app(2);
    app.add_systems(Startup, spawn_counter)
        .rollback_component_with_copy::<Counter>()
        .checksum_component_with_hash::<Counter>()
        .add_systems(GgrsSchedule, non_deterministic_counter);

    app.init_resource::<MismatchDetected>();
    app.world_mut().add_observer(
        |_trigger: On<SyncTestMismatch>, mut detected: ResMut<MismatchDetected>| {
            detected.0 = true;
        },
    );

    for _ in 0..10 {
        app.update();
    }

    assert!(
        app.world().resource::<MismatchDetected>().0,
        "SyncTestMismatch should have fired due to non-deterministic game logic"
    );
}

/// Verifies that `ConfirmedFrameCount` advances for SyncTest sessions and that old snapshots are
/// pruned once confirmed. Regression test for the inverted confirmed frame condition bug.
#[test]
fn synctest_prunes_confirmed_snapshots() {
    let check_distance: usize = 5;
    let mut app = create_app(check_distance);

    // Run enough frames for ConfirmedFrameCount to advance well past 0
    for _ in 0..20 {
        app.update();
    }

    let confirmed = app.world().resource::<ConfirmedFrameCount>().0;
    assert!(
        confirmed > 0,
        "ConfirmedFrameCount should advance for SyncTest sessions, got {confirmed}"
    );

    // Snapshots at or before the confirmed frame should have been pruned
    let snapshots = app
        .world()
        .resource::<GgrsResourceSnapshots<FrameCounter>>();
    assert!(
        snapshots.peek(0).is_none(),
        "Frame 0 snapshot should have been pruned (confirmed_frame={confirmed})"
    );
}

/// Verifies that entities without the `Rollback` marker are not touched by rollback
/// and don't cause a panic when rollback-enabled entities are also present.
#[test]
fn non_rollback_entity_survives_rollback() {
    #[derive(Component)]
    struct NonRollbackMarker;

    let mut app = base_synctest_app(2);
    app.add_systems(Startup, |mut commands: Commands| {
        // This entity has no Rollback component — bevy_ggrs should leave it alone.
        commands.spawn(NonRollbackMarker);
    });

    // Run through enough updates to trigger several rollbacks; the test passes if no panic occurs.
    for _ in 0..20 {
        app.update();
    }

    assert!(
        app.world_mut()
            .query::<&NonRollbackMarker>()
            .iter(app.world())
            .count()
            == 1,
        "Non-rollback entity should survive intact through all updates"
    );
}

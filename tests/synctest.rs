use bevy::{platform::collections::HashMap, prelude::*, time::TimeUpdateStrategy};
use bevy_ggrs::{prelude::*, *};
use core::time::Duration;
use ggrs::*;

pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = u8;
    type State = u8;
    type Address = usize;
}

#[derive(Reflect, Resource, Default, Debug, Clone)]
struct FrameCounter(u16);

fn frame_counter(mut counter: ResMut<FrameCounter>) {
    counter.0 = counter.0.wrapping_add(1);
}

fn input_system(mut commands: Commands, players: Res<LocalPlayers>) {
    let mut inputs = HashMap::new();
    for &handle in &players.0 {
        inputs.insert(handle, 0u8);
    }
    commands.insert_resource(LocalInputs::<GgrsConfig>(inputs));
}

fn create_app(check_distance: usize) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .init_resource::<FrameCounter>()
        .insert_resource(Session::SyncTest(
            SessionBuilder::<GgrsConfig>::new()
                .with_num_players(1)
                .unwrap()
                .with_check_distance(check_distance)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, input_system)
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
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(Session::SyncTest(
            SessionBuilder::<GgrsConfig>::new()
                .with_num_players(1)
                .unwrap()
                .with_check_distance(5)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, input_system)
        .rollback_component_with_copy::<Health>()
        .add_systems(Startup, spawn_player)
        .add_systems(GgrsSchedule, decrease_health);
    app
}

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

/// Verifies that `ConfirmedFrameCount` advances for SyncTest sessions and that old snapshots are
/// pruned once confirmed. Regression test for the inverted confirmed frame condition bug.
#[test]
fn synctest_prunes_confirmed_snapshots() {
    let check_distance: usize = 5;
    let mut app = create_app(check_distance);
    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

    // Run enough frames for ConfirmedFrameCount to advance well past 0
    for _ in 0..20 {
        sleep();
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

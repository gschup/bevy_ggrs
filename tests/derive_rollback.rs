use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_ggrs::*;
use core::time::Duration;
use ggrs::*;

pub struct TestConfig;
impl Config for TestConfig {
    type Input = u8;
    type State = u8;
    type Address = usize;
    type InputPredictor = ggrs::PredictRepeatLast;
}

// Test the derive macro with explicit app.rollback::<T>() path
#[derive(Component, Clone, Copy, Rollback)]
#[component(summary_tick)]
#[rollback(copy, marker)]
struct Player {
    score: u32,
}

#[derive(Resource, Clone, Rollback, Default)]
#[rollback(resource, clone)]
struct GameState {
    round: u32,
}

fn input_system(mut commands: Commands) {
    let mut local_inputs = HashMap::new();
    local_inputs.insert(0, 0u8);
    commands.insert_resource(LocalInputs::<TestConfig>(local_inputs));
}

fn setup_system(mut commands: Commands) {
    commands.spawn(Player { score: 0 });
}

fn advance_score(mut query: Query<&mut Player>) {
    for mut player in &mut query {
        player.score += 10;
    }
}

fn advance_round(mut state: ResMut<GameState>) {
    state.round += 1;
}

// Auto-registration types: Reflect + Rollback + #[reflect(Rollback)]
// These should be auto-discovered by SnapshotPlugin::finish()
#[derive(Component, Clone, Copy, Reflect, Rollback)]
#[component(summary_tick)]
#[rollback(copy, marker)]
#[reflect(Rollback)]
struct AutoPlayer {
    health: u32,
}

#[derive(Resource, Clone, Reflect, Rollback, Default)]
#[rollback(resource, clone)]
#[reflect(Rollback)]
struct AutoScore {
    value: u32,
}

fn auto_setup(mut commands: Commands) {
    commands.spawn(AutoPlayer { health: 100 });
}

fn damage_player(mut query: Query<&mut AutoPlayer>) {
    for mut player in &mut query {
        player.health = player.health.saturating_sub(1);
    }
}

fn increment_score(mut score: ResMut<AutoScore>) {
    score.value += 1;
}

/// Test that types with #[reflect(Rollback)] are auto-discovered and registered
/// without any explicit app.rollback::<T>() calls.
#[test]
fn derive_rollback_auto_registration() {
    let mut app = App::new();

    // No skip_auto_registration, no explicit rollback::<T>() calls
    app.add_plugins(MinimalPlugins)
        .add_plugins(GgrsPlugin::<TestConfig>::default())
        .insert_resource(Session::SyncTest(
            SessionBuilder::<TestConfig>::new()
                .with_num_players(1)
                .unwrap()
                .with_check_distance(2)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .init_resource::<AutoScore>()
        .add_systems(ReadInputs, input_system)
        .add_systems(Startup, auto_setup)
        .add_systems(GgrsSchedule, (damage_player, increment_score));

    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

    app.update();

    for _ in 0..10 {
        sleep();
        app.update();
    }

    // If auto-registration worked, rollback save/load ran without panicking
    let player = app
        .world_mut()
        .query::<&AutoPlayer>()
        .single(app.world())
        .unwrap();
    assert!(player.health < 100, "Player health should have decreased");

    let score = app.world().resource::<AutoScore>();
    assert!(score.value > 0, "Score should have increased");
}

/// Test that the derive macro generates a working RollbackRegistration impl,
/// and that app.rollback::<T>() registers everything correctly.
#[test]
fn derive_rollback_explicit_path() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins)
        .add_plugins(GgrsPlugin::<TestConfig>::default())
        .skip_auto_registration()
        .insert_resource(Session::SyncTest(
            SessionBuilder::<TestConfig>::new()
                .with_num_players(1)
                .unwrap()
                .with_check_distance(2)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .init_resource::<GameState>()
        .rollback::<Player>()
        .rollback::<GameState>()
        .add_systems(ReadInputs, input_system)
        .add_systems(Startup, setup_system)
        .add_systems(GgrsSchedule, (advance_score, advance_round));

    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

    // Initial update to run Startup
    app.update();

    // Run several frames so SyncTest exercises rollback
    for _ in 0..10 {
        sleep();
        app.update();
    }

    // If we got here without panicking, the rollback save/load cycle works.
    // Verify the game is progressing.
    let player = app
        .world_mut()
        .query::<&Player>()
        .single(app.world())
        .unwrap();
    assert!(player.score > 0, "Player score should have advanced");

    let state = app.world().resource::<GameState>();
    assert!(state.round > 0, "Game round should have advanced");
}

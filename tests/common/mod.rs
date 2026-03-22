use bevy::{platform::collections::HashMap, prelude::*, time::TimeUpdateStrategy};
use bevy_ggrs::{prelude::*, *};
use core::time::Duration;
use ggrs::*;

/// Minimal GGRS config used across all SyncTest-based integration tests.
pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = u8;
    type State = u8;
    type Address = usize;
}

/// Fills `LocalInputs` with zero-input for every local player. Used as the `ReadInputs` system
/// in tests that don't need meaningful input.
pub fn input_system(mut commands: Commands, players: Res<LocalPlayers>) {
    let mut inputs = HashMap::new();
    for &handle in &players.0 {
        inputs.insert(handle, 0u8);
    }
    commands.insert_resource(LocalInputs::<GgrsConfig>(inputs));
}

/// Builds a single-player `SyncTestSession` with the given check distance.
pub fn synctest_session(check_distance: usize) -> Session<GgrsConfig> {
    Session::SyncTest(
        SessionBuilder::<GgrsConfig>::new()
            .with_num_players(1)
            .unwrap()
            .with_check_distance(check_distance)
            .add_player(PlayerType::Local, 0)
            .unwrap()
            .start_synctest_session()
            .unwrap(),
    )
}

/// Returns a minimal `App` configured for SyncTest rollback testing:
/// `MinimalPlugins`, manual 60 FPS time step, a single-player SyncTest session with the
/// given check distance, `GgrsPlugin`, and the no-op `input_system` in `ReadInputs`.
///
/// Call additional builder methods on the returned `App` to register rollback components,
/// add startup/game-logic systems, etc.
pub fn base_synctest_app(check_distance: usize) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(synctest_session(check_distance))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, input_system);
    app
}

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*, utils::HashMap};
use bevy_ggrs::{
    ggrs::{Config, PlayerType, SessionBuilder},
    prelude::*,
    LocalInputs, LocalPlayers,
};

#[derive(Debug)]
pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = u8;
    type State = u8;
    type Address = String;
}

pub fn read_local_input(mut commands: Commands, local_players: Res<LocalPlayers>) {
    dbg!(&local_players.0);
    let mut local_inputs = HashMap::new();
    for handle in &local_players.0 {
        local_inputs.insert(*handle, 0);
    }
    commands.insert_resource(LocalInputs::<GgrsConfig>(local_inputs));
}

/// Player health
#[derive(Component, Hash, Debug, Clone, Copy)]
pub struct Health(u32);

impl Default for Health {
    fn default() -> Self {
        Self(10)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = SessionBuilder::<GgrsConfig>::new()
        .with_num_players(1)
        // each frame, roll back and resimulate 5 frames back in time, and compare checksums
        .with_check_distance(5);
    session = session.add_player(PlayerType::Local, 0)?;
    let session = session.start_synctest_session()?;

    App::new()
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .set_rollback_schedule_fps(60)
        .add_systems(ReadInputs, read_local_input)
        .rollback_component_with_copy::<Health>()
        .checksum_component_with_hash::<Health>()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 10.0,
            ))),
            LogPlugin::default(),
        ))
        .add_systems(Startup, spawn_player)
        .add_systems(GgrsSchedule, decrease_health)
        .insert_resource(Session::SyncTest(session))
        .run();

    Ok(())
}

fn spawn_player(mut commands: Commands) {
    info!("spawning player");
    commands.spawn(Health::default()).add_rollback();
}

fn decrease_health(mut commands: Commands, mut players: Query<(Entity, &mut Health)>) {
    for (player_entity, mut health) in &mut players {
        health.0 = health.0.saturating_sub(1);
        info!("{health:?}");

        if health.0 == 0 {
            info!("despawning player");
            commands.entity(player_entity).despawn_recursive();
        }
    }
}

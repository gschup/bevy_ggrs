use bevy::prelude::*;
use bevy_ggrs::prelude::*;
use clap::Parser;
use ggrs::UdpNonBlockingSocket;
use std::net::SocketAddr;

mod box_game;
use box_game::*;

const FPS: usize = 60;

// clap will read command line arguments
#[derive(Parser, Resource)]
struct Opt {
    #[clap(short, long)]
    local_port: u16,
    #[clap(short, long)]
    num_players: usize,
    #[clap(long)]
    host: SocketAddr,
}

#[derive(Resource)]
struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::parse();
    assert!(opt.num_players > 0);

    // create a GGRS session

    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port)?;
    let sess = SessionBuilder::<BoxConfig>::new()
        .with_num_players(opt.num_players)
        .start_spectator_session(opt.host, socket);

    App::new()
        .add_plugins(GgrsPlugin::<BoxConfig>::default())
        // define frequency of rollback game logic update
        .set_rollback_schedule_fps(FPS)
        // this system will be executed as part of input reading
        .add_systems(ReadInputs, read_local_inputs)
        // Rollback behavior can be customized using a variety of extension methods and plugins:
        // The FrameCount resource implements Copy, we can use that to have minimal overhead rollback
        .rollback_resource_with_copy::<FrameCount>()
        // Same with the Velocity Component
        .rollback_component_with_copy::<Velocity>()
        // Transform only implement Clone, so instead we'll use that to snapshot and rollback with
        .rollback_component_with_clone::<Transform>()
        .insert_resource(opt)
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup_system)
        // these systems will be executed as part of the advance frame update
        .add_systems(GgrsSchedule, (move_cube_system, increase_frame_system))
        // add your GGRS session
        .insert_resource(Session::Spectator(sess))
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        // print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(Update, print_network_stats_system)
        .add_systems(Update, print_events_system)
        .run();

    Ok(())
}

fn print_events_system(mut session: ResMut<Session<BoxConfig>>) {
    match session.as_mut() {
        Session::Spectator(s) => {
            for event in s.events() {
                println!("GGRS Event: {:?}", event);
            }
        }
        _ => panic!("This example focuses on spectators."),
    }
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    p2p_session: Option<Res<Session<BoxConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(sess) = p2p_session {
            match sess.as_ref() {
                Session::Spectator(s) => {
                    if let Ok(stats) = s.network_stats() {
                        println!("NetworkStats : {:?}", stats);
                    }
                }
                _ => panic!("This example focuses on spectators."),
            }
        }
    }
}

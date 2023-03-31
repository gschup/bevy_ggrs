use std::net::SocketAddr;

use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{AdvanceFrame, GgrsApp, GgrsPlugin, Session};
use ggrs::{SessionBuilder, UdpNonBlockingSocket};

use structopt::StructOpt;

mod box_game;
use box_game::*;

const FPS: usize = 60;

// structopt will read command line parameters for u
#[derive(StructOpt, Resource)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    host: SocketAddr,
}

#[derive(Resource)]
struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    assert!(opt.num_players > 0);

    // create a GGRS session

    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port)?;
    let sess = SessionBuilder::<GGRSConfig>::new()
        .with_num_players(opt.num_players)
        .start_spectator_session(opt.host, socket);

    // build the bevy app
    App::new()
        .insert_resource(opt)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(720., 720.),
                title: "GGRS Box Game".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .add_plugin(GgrsPlugin::<GGRSConfig>::default())
        // set the FPS we want our rollback schedule to run with
        .set_rollback_schedule_fps(FPS)
        // register types we want saved and loaded when rollbacking
        .register_rollback_component::<Transform>()
        .register_rollback_component::<Velocity>()
        .register_rollback_resource::<FrameCount>()
        // these systems will be executed as part of the advance frame update
        .add_systems((move_cube_system, increase_frame_system).in_schedule(AdvanceFrame))
        // add your GGRS session
        .insert_resource(Session::SpectatorSession(sess))
        // insert a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        // not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        // setup for the scene
        .add_startup_system(setup_system)
        // debug prints
        .add_system(print_network_stats_system)
        .add_system(print_events_system)
        .run();
    Ok(())
}

fn print_events_system(mut session: ResMut<Session<GGRSConfig>>) {
    match session.as_mut() {
        Session::SpectatorSession(s) => {
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
    p2p_session: Option<Res<Session<GGRSConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(sess) = p2p_session {
            match sess.as_ref() {
                Session::SpectatorSession(s) => {
                    if let Ok(stats) = s.network_stats() {
                        println!("NetworkStats : {:?}", stats);
                    }
                }
                _ => panic!("This example focuses on spectators."),
            }
        }
    }
}

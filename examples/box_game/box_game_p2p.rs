use std::net::SocketAddr;

use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{GgrsAppExtension, GgrsPlugin, GgrsSchedule, Session};
use ggrs::{GGRSEvent as GgrsEvent, PlayerType, SessionBuilder, UdpNonBlockingSocket};

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
    players: Vec<String>,
    #[structopt(short, long)]
    spectators: Vec<SocketAddr>,
}

#[derive(Resource)]
struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // create a GGRS session
    let mut sess_build = SessionBuilder::<GgrsConfig>::new()
        .with_num_players(num_players)
        .with_desync_detection_mode(ggrs::DesyncDetection::On { interval: 10 }) // (optional) set how often to exchange state checksums
        .with_max_prediction_window(12) // (optional) set max prediction window
        .with_input_delay(2); // (optional) set input delay for the local player

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            sess_build = sess_build.add_player(PlayerType::Local, i)?;
        } else {
            // remote players
            let remote_addr: SocketAddr = player_addr.parse()?;
            sess_build = sess_build.add_player(PlayerType::Remote(remote_addr), i)?;
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        sess_build = sess_build.add_player(PlayerType::Spectator(*spec_addr), num_players + i)?;
    }

    // start the GGRS session
    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port)?;
    let sess = sess_build.start_p2p_session(socket)?;

    App::new()
        .add_ggrs_plugin(
            GgrsPlugin::<GgrsConfig>::new()
                // define frequency of rollback game logic update
                .with_update_frequency(FPS)
                // define system that returns inputs given a player handle, so GGRS can send the inputs around
                .with_input_system(input)
                // register types of components AND resources you want to be rolled back
                .register_rollback_component::<Transform>()
                .register_rollback_component::<Velocity>()
                .register_rollback_resource::<FrameCount>(),
        )
        .insert_resource(opt)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(720., 720.),
                title: "GGRS Box Game".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_system)
        // these systems will be executed as part of the advance frame update
        .add_systems(GgrsSchedule, (move_cube_system, increase_frame_system))
        // add your GGRS session
        .insert_resource(Session::P2P(sess))
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        //print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(Update, print_network_stats_system)
        .add_systems(Update, print_events_system)
        .run();

    Ok(())
}

fn print_events_system(mut session: ResMut<Session<GgrsConfig>>) {
    match session.as_mut() {
        Session::P2P(s) => {
            for event in s.events() {
                match event {
                    GgrsEvent::Disconnected { .. } | GgrsEvent::NetworkInterrupted { .. } => {
                        warn!("GGRS event: {event:?}")
                    }
                    GgrsEvent::DesyncDetected { .. } => error!("GGRS event: {event:?}"),
                    _ => info!("GGRS event: {event:?}"),
                }
            }
        }
        _ => panic!("This example focuses on p2p."),
    }
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    p2p_session: Option<Res<Session<GgrsConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(sess) = p2p_session {
            match sess.as_ref() {
                Session::P2P(s) => {
                    let num_players = s.num_players();
                    for i in 0..num_players {
                        if let Ok(stats) = s.network_stats(i) {
                            println!("NetworkStats for player {}: {:?}", i, stats);
                        }
                    }
                }
                _ => panic!("This examples focuses on p2p."),
            }
        }
    }
}

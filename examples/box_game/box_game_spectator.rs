use std::net::SocketAddr;

use bevy::prelude::*;
use bevy_ggrs::{GGRSPlugin, SessionType};
use ggrs::{SessionBuilder, SpectatorSession, UdpNonBlockingSocket};
use structopt::StructOpt;

mod box_game;
use box_game::*;

const FPS: usize = 60;
const ROLLBACK_DEFAULT: &str = "rollback_default";

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    host: SocketAddr,
}

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

    let mut app = App::new();
    GGRSPlugin::<GGRSConfig>::new()
        // define frequency of rollback game logic update
        .with_update_frequency(FPS)
        // define system that returns inputs given a player handle, so GGRS can send the inputs around
        .with_input_system(input)
        // register types of compontents AND resources you want to be rolled back
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        .register_rollback_type::<FrameCount>()
        // these systems will be executed as part of the advance frame update
        .with_rollback_schedule(
            Schedule::default().with_stage(
                ROLLBACK_DEFAULT,
                SystemStage::parallel()
                    .with_system(move_cube_system)
                    .with_system(increase_frame_system),
            ),
        )
        // make it happen in the bevy app
        .build(&mut app);

    // continue building/running the app like you normally would
    app.insert_resource(Msaa { samples: 4 })
        .insert_resource(WindowDescriptor {
            width: 720.,
            height: 720.,
            title: "GGRS Box Game".to_owned(),
            ..Default::default()
        })
        .insert_resource(opt)
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup_system)
        // add your GGRS session
        .insert_resource(sess)
        .insert_resource(SessionType::SpectatorSession)
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        //print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(2.0, true)))
        .add_system(print_network_stats_system)
        .add_system(print_events_system)
        .run();

    Ok(())
}

fn print_events_system(mut session: ResMut<SpectatorSession<GGRSConfig>>) {
    for event in session.events() {
        println!("GGRS Event: {:?}", event);
    }
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    p2p_session: Option<Res<SpectatorSession<GGRSConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(sess) = p2p_session {
            if let Ok(stats) = sess.network_stats() {
                println!("NetworkStats : {:?}", stats);
            }
        }
    }
}

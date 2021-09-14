use std::net::SocketAddr;

use bevy::prelude::*;
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use ggrs::{P2PSession, PlayerType};
use structopt::StructOpt;

mod box_game;
use box_game::*;

const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const FPS: u32 = 60;

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    players: Vec<String>,
    #[structopt(short, long)]
    spectators: Vec<SocketAddr>,
}

struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // create a GGRS P2P session
    let mut p2p_sess = P2PSession::new(num_players as u32, INPUT_SIZE, opt.local_port)?;

    // set default expected update frequency (affects synchronization timings between players)
    p2p_sess.set_fps(FPS).expect("Invalid fps");

    // turn on sparse saving
    p2p_sess.set_sparse_saving(true)?;

    App::new()
        .insert_resource(Msaa { samples: 4 })
        .insert_resource(opt)
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        .add_startup_system(start_p2p_session)
        .add_startup_system(setup_system)
        // add your GGRS session
        .with_p2p_session(p2p_sess)
        // define frequency of rollback game logic update
        .with_fps(FPS)
        // define system that represents your inputs as a byte vector, so GGRS can send the inputs around
        .with_input_system(input.system())
        // register components that will be loaded/saved
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        // you can also register resources
        .insert_resource(FrameCount { frame: 0 })
        .register_rollback_type::<FrameCount>()
        // these systems will be executed as part of the advance frame update
        .add_rollback_system(move_cube_system)
        .add_rollback_system(increase_frame_system)
        //print some network stats
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(2.0, true)))
        .add_system(print_network_stats_system)
        .run();

    Ok(())
}

fn start_p2p_session(mut p2p_sess: ResMut<P2PSession>, opt: Res<Opt>) {
    let mut local_handle = 0;
    let num_players = p2p_sess.num_players() as usize;

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            p2p_sess.add_player(PlayerType::Local, i).unwrap();
            local_handle = i;
        } else {
            // remote players
            let remote_addr: SocketAddr =
                player_addr.parse().expect("Invalid remote player address");
            p2p_sess
                .add_player(PlayerType::Remote(remote_addr), i)
                .unwrap();
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        p2p_sess
            .add_player(PlayerType::Spectator(*spec_addr), num_players + i)
            .unwrap();
    }

    // set input delay for the local player
    p2p_sess.set_frame_delay(2, local_handle).unwrap();

    // start the GGRS session
    p2p_sess.start_session().unwrap();
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    p2p_session: Option<Res<P2PSession>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(sess) = p2p_session {
            let num_players = sess.num_players() as usize;
            for i in 0..num_players {
                if let Ok(stats) = sess.network_stats(i) {
                    println!("NetworkStats for player {}: {:?}", i, stats);
                }
            }
        }
    }
}

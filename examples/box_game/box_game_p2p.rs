use std::net::SocketAddr;

use bevy::{core::FixedTimestep, prelude::*};
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use ggrs::PlayerType;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    let mut local_handle = 0;
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // create a GGRS P2P session
    let mut p2p_sess = ggrs::start_p2p_session(num_players as u32, INPUT_SIZE, opt.local_port)?;

    // turn on sparse saving
    p2p_sess.set_sparse_saving(true)?;

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            p2p_sess.add_player(PlayerType::Local, i)?;
            local_handle = i;
        } else {
            // remote players
            let remote_addr: SocketAddr = player_addr.parse()?;
            p2p_sess.add_player(PlayerType::Remote(remote_addr), i)?;
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        p2p_sess.add_player(PlayerType::Spectator(*spec_addr), num_players + i)?;
    }

    // set input delay for the local player
    p2p_sess.set_frame_delay(2, local_handle)?;

    // set change default expected update frequency (affects synchronization timings between players)
    p2p_sess.set_fps(FPS)?;

    // start the GGRS session
    p2p_sess.start_session()?;

    App::new()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        .add_startup_system(setup_system)
        // add your GGRS session
        .with_p2p_session(p2p_sess)
        // define frequency of game logic update
        .with_rollback_run_criteria(FixedTimestep::steps_per_second(60.0))
        // define system that creates a compact input representation
        .with_input_system(input.system())
        // register components that will be loaded/saved
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        // you can also register resources
        .register_rollback_type::<FrameCount>()
        // these systems will be executed as part of the advance frame update
        .add_rollback_system(move_cube_system)
        .add_rollback_system(increase_frame_system)
        .run();

    Ok(())
}

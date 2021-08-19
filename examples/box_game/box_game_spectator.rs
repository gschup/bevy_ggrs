use std::net::SocketAddr;

use bevy::{core::FixedTimestep, prelude::*};
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use structopt::StructOpt;

mod box_game;
use box_game::*;

const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const FPS: u32 = 60;

// structopt will read command line parameters for us
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    host: SocketAddr,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    assert!(opt.num_players > 0);

    // create a GGRS session for a spectator
    let mut spec_sess = ggrs::start_p2p_spectator_session(
        opt.num_players as u32,
        INPUT_SIZE,
        opt.local_port,
        opt.host,
    )?;

    // change catch-up parameters, if desired
    spec_sess.set_max_frames_behind(5)?; // when the spectator is more than this amount of frames behind, it will catch up
    spec_sess.set_catchup_speed(2)?; // set this to 1 if you don't want any catch-ups

    // set change default expected update frequency (not super important in the spectator session)
    spec_sess.set_fps(FPS)?;

    // start the GGRS session
    spec_sess.start_session()?;

    App::new()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        .add_startup_system(setup_system)
        // add your GGRS session
        .with_p2p_spectator_session(spec_sess)
        // define frequency of game logic update
        .with_rollback_run_criteria(FixedTimestep::steps_per_second(FPS as f64))
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

use bevy::prelude::*;
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use ggrs::SyncTestSession;
use structopt::StructOpt;

mod box_game;
use box_game::*;

const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const FPS: u32 = 60;

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    check_distance: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();

    // start a GGRS SyncTest session, which will simulate rollbacks every frame
    let mut sync_sess =
        SyncTestSession::new(opt.num_players as u32, INPUT_SIZE, opt.check_distance)?;

    // set input delay for any player you want (if you want)
    for i in 0..opt.num_players {
        sync_sess.set_frame_delay(2, i)?;
    }

    App::new()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        .add_startup_system(setup_system)
        // add your GGRS session
        .with_synctest_session(sync_sess)
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
        .run();

    Ok(())
}

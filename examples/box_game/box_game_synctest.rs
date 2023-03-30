use bevy::prelude::*;
use bevy_ggrs::{GGRSPlugin, GgrsSchedule, Session};
use ggrs::{PlayerType, SessionBuilder};
use structopt::StructOpt;

mod box_game;
use box_game::*;

const FPS: usize = 60;

// structopt will read command line parameters for u
#[derive(StructOpt, Resource)]
struct Opt {
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    check_distance: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    assert!(opt.num_players > 0);

    // create a GGRS session
    let mut sess_build = SessionBuilder::<GGRSConfig>::new()
        .with_num_players(opt.num_players)
        .with_check_distance(opt.check_distance)
        .with_input_delay(2); // (optional) set input delay for the local player

    // add players
    for i in 0..opt.num_players {
        sess_build = sess_build.add_player(PlayerType::Local, i)?;
    }

    // start the GGRS session
    let sess = sess_build.start_synctest_session()?;

    let mut app = App::new();
    GGRSPlugin::<GGRSConfig>::new()
        // define frequency of rollback game logic update
        .with_update_frequency(FPS)
        // define system that returns inputs given a player handle, so GGRS can send the inputs around
        .with_input_system(input)
        // register types of components AND resources you want to be rolled back
        .register_rollback_component::<Transform>()
        .register_rollback_component::<Velocity>()
        .register_rollback_resource::<FrameCount>()
        // make it happen in the bevy app
        .build(&mut app);

    // continue building/running the app like you normally would
    app.insert_resource(opt)
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup_system)
        // these systems will be executed as part of the advance frame update
        .add_systems((move_cube_system, increase_frame_system).in_schedule(GgrsSchedule))
        // add your GGRS session
        .insert_resource(Session::SyncTestSession(sess))
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        .run();

    Ok(())
}

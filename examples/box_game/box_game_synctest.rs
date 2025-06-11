use bevy::prelude::*;
use bevy_ggrs::{prelude::*, RollbackFrameRate};
use clap::Parser;

mod box_game;
use box_game::*;

const FPS: usize = 60;

// clap will read command line arguments
#[derive(Parser, Resource)]
struct Opt {
    #[clap(short, long)]
    num_players: usize,
    #[clap(short, long)]
    check_distance: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::parse();
    assert!(opt.num_players > 0);

    // create a GGRS session
    let mut sess_build = SessionBuilder::<BoxConfig>::new()
        .with_num_players(opt.num_players)
        .with_check_distance(opt.check_distance)
        .with_input_delay(2); // (optional) set input delay for the local player

    // add players
    for i in 0..opt.num_players {
        sess_build = sess_build.add_player(PlayerType::Local, i)?;
    }

    // start the GGRS session
    let sess = sess_build.start_synctest_session()?;

    App::new()
        .add_plugins(GgrsPlugin::<BoxConfig>::default())
        // define frequency of rollback game logic update
        .insert_resource(RollbackFrameRate(FPS))
        // this system will be executed as part of input reading
        .add_systems(ReadInputs, read_local_inputs)
        .insert_resource(opt)
        .add_plugins(DefaultPlugins)
        // Rollback behavior can be customized using a variety of extension methods and plugins:
        // The FrameCount resource implements Copy, we can use that to have minimal overhead rollback
        .rollback_resource_with_copy::<FrameCount>()
        // Same with the Velocity Component
        .rollback_component_with_copy::<Velocity>()
        // Transform only implement Clone, so instead we'll use that to snapshot and rollback with
        .rollback_component_with_clone::<Transform>()
        .add_systems(Startup, setup_system)
        // these systems will be executed as part of the advance frame update
        .add_systems(GgrsSchedule, (move_cube_system, increase_frame_system))
        // add your GGRS session
        .insert_resource(Session::SyncTest(sess))
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        .run();

    Ok(())
}

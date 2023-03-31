use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{AdvanceFrame, GgrsApp, GgrsPlugin, ReadInputs, Session};
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

    // remember local player handles - in synctest, all players are local
    let local_players = (0..opt.num_players).collect();

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
        // this system will be executed as part of input reading
        .add_system(read_local_inputs.in_schedule(ReadInputs))
        .insert_resource(LocalPlayers(local_players))
        // these systems will be executed as part of the advance frame update
        .add_systems((move_cube_system, increase_frame_system).in_schedule(AdvanceFrame))
        // add your GGRS session
        .insert_resource(Session::SyncTestSession(sess))
        // insert a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        // setup for the scene
        .add_startup_system(setup_system)
        .run();

    Ok(())
}

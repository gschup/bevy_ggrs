use bevy::prelude::*;
use bevy_ggrs::{GGRSApp, GGRSPlugin, Rollback};
use ggrs::SyncTestSession;
use structopt::StructOpt;

mod box_game;
use box_game::*;

const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const MAX_ROLLBACK_LENGTH: usize = 16;
const FPS: u32 = 60;
const ROLLBACK_DEFAULT: &str = "rollback_default";
const CHECKSUM_STAGE: &str = "checksum";

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    num_players: usize,
    #[structopt(short, long)]
    check_distance: usize,
}

// If your Component / Resource implements Hash, you can make use of `#[reflect(Hash)]`
// in order to allow a GGRS `SyncTestSession` to construct a checksum for a world snapshot
// Here we create a general purpose checksum component to construct a checksum for types that cannot make use of `#[reflect(Hash)]` like bevy::Transform
// You can uncomment the non-deterministic code in move_cube_system (box_game.rs) to test checksum mismatches.
#[derive(Default, Reflect, Hash, Component)]
#[reflect(Hash)]
struct Checksum {
    value: u64,
}

// add checksum component to each player
fn setup_checksum_system(
    mut commands: Commands,
    mut query: Query<Entity, (With<Player>, With<Rollback>, Without<Checksum>)>,
) {
    for entity in query.iter_mut() {
        commands.entity(entity).insert(Checksum::default());
    }
}

// computes the checksum of the transform component
fn compute_checksum_system(
    mut query: Query<(&Transform, &mut Checksum), (With<Player>, With<Rollback>)>,
) {
    let n = 17;
    let p1 = 73;
    let p2 = 1433;
    let p3 = 2371;

    for (transform, mut checksum) in query.iter_mut() {
        let x = transform.translation.x.floor() as i32;
        let y = transform.translation.y.floor() as i32;
        let z = transform.translation.z.floor() as i32;

        // naive Vec3 checksum implementation
        checksum.value = (((x * p1) ^ (y * p2) ^ (z * p3)) % n) as u64;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();

    // start a GGRS SyncTest session, which will simulate rollbacks every frame
    let mut sync_sess = SyncTestSession::new(
        opt.num_players as u32,
        INPUT_SIZE,
        MAX_ROLLBACK_LENGTH,
        opt.check_distance,
    )?;

    // set input delay for any player you want (if you want)
    for i in 0..opt.num_players {
        sync_sess.set_frame_delay(2, i)?;
    }

    App::new()
        .insert_resource(Msaa { samples: 4 })
        .insert_resource(WindowDescriptor {
            width: 720.,
            height: 720.,
            title: "GGRS Box Game".to_owned(),
            vsync: false,
            ..Default::default()
        })
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        // setup game system and checksum system in that order
        // setup_checksum_system will add a checksum component for each player
        .add_startup_system_to_stage(StartupStage::Startup, setup_system)
        .add_startup_system_to_stage(StartupStage::PostStartup, setup_checksum_system)
        // add your GGRS session
        .with_synctest_session(sync_sess)
        // define frequency of rollback game logic update
        .with_update_frequency(FPS)
        // define system that represents your inputs as a byte vector, so GGRS can send the inputs around
        .with_input_system(input)
        // register components that will be loaded/saved
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        .register_rollback_type::<Checksum>()
        // you can also insert resources that will be loaded/saved
        .insert_rollback_resource(FrameCount { frame: 0 })
        // these systems will be executed as part of the advance frame update
        .with_rollback_schedule(
            Schedule::default()
                .with_stage(
                    ROLLBACK_DEFAULT,
                    SystemStage::single_threaded()
                        .with_system(move_cube_system)
                        .with_system(increase_frame_system),
                )
                .with_stage_after(
                    ROLLBACK_DEFAULT,
                    CHECKSUM_STAGE,
                    SystemStage::parallel().with_system(compute_checksum_system),
                ),
        )
        .run();

    Ok(())
}

use std::net::SocketAddr;

use bevy::{core::FixedTimestep, prelude::*};
use bevy_ggrs::{GGRSAppBuilder, GGRSPlugin, Rollback, RollbackIdProvider};
use ggrs::{GameInput, P2PSession, PlayerHandle, PlayerType};
use structopt::StructOpt;

const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const PLAYER_COLORS: [Color; 2] = [Color::rgb(0.8, 0.6, 0.2), Color::rgb(0., 0.35, 0.8)];
const FPS: u32 = 60;

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;

const MOVEMENT_SPEED: f32 = 0.005;
const MAX_SPEED: f32 = 0.1;
const FRICTION: f32 = 0.9;
const PLANE_SIZE: f32 = 5.0;
const CUBE_SIZE: f32 = 0.2;

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

struct Player {
    handle: u32,
}

// Components that should be saved/loaded need to implement the `Reflect` trait
#[derive(Default, Reflect)]
#[reflect(Component)]
struct Velocity {
    x: f32,
    y: f32,
    z: f32,
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

    App::build()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        .add_startup_system(setup.system())
        // add your GGRS session
        .with_p2p_session(p2p_sess)
        // define frequency of game logic update
        .with_rollback_run_criteria(FixedTimestep::steps_per_second(60.0))
        // define system that creates a compact input representation
        .with_input_system(input.system())
        // register components that will be loaded/saved
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        // these systems will be executed as part of the advance frame update
        .add_rollback_system(move_cube.system())
        .run();

    Ok(())
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    session: Res<P2PSession>,
) {
    // plane
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane { size: PLANE_SIZE })),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..Default::default()
    });

    // player cube - just spawn whatever entity you want, then add a `Rollback` component with a unique id (for example through the `RollbackIdProvider` resource).
    // Every entity that you want to be saved/loaded needs a `Rollback` component with a unique rollback id.
    // When loading entities from the past, this extra id is necessary to connect entities over different game states
    let r = PLANE_SIZE / 4.;

    for handle in 0..session.num_players() {
        let rot = handle as f32 / session.num_players() as f32 * 2. * std::f32::consts::PI;
        let x = r * rot.cos();
        let z = r * rot.sin();

        let mut transform = Transform::default();
        transform.translation.x = x;
        transform.translation.y = CUBE_SIZE / 2.;
        transform.translation.z = z;

        commands
            .spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::Cube { size: CUBE_SIZE })),
                material: materials.add(PLAYER_COLORS[handle as usize].into()),
                transform,
                ..Default::default()
            })
            .insert(Player { handle })
            .insert(Velocity::default())
            .insert(Rollback::new(rip.next_id()));
    }

    // light
    commands.spawn_bundle(LightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..Default::default()
    });
    // camera
    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    });
}

// System that moves the cubes, added as a rollback system above.
// Filtering for the rollback component is a good way to make sure your game logic systems
// only mutate components that are being saved/loaded.
fn move_cube(
    mut query: Query<(&mut Transform, &mut Velocity, &Player), With<Rollback>>,
    inputs: Res<Vec<GameInput>>,
) {
    for (mut t, mut v, p) in query.iter_mut() {
        let input = inputs[p.handle as usize].buffer[0];
        // set velocity through key presses
        if input & INPUT_UP != 0 && input & INPUT_DOWN == 0 {
            v.z -= MOVEMENT_SPEED;
        }
        if input & INPUT_UP == 0 && input & INPUT_DOWN != 0 {
            v.z += MOVEMENT_SPEED;
        }
        if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
            v.x -= MOVEMENT_SPEED;
        }
        if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
            v.x += MOVEMENT_SPEED;
        }

        // slow down
        if input & INPUT_UP == 0 && input & INPUT_DOWN == 0 {
            v.x *= FRICTION;
        }
        if input & INPUT_LEFT == 0 && input & INPUT_RIGHT == 0 {
            v.z *= FRICTION;
        }
        v.y *= FRICTION;

        // constrain velocity
        v.x = v.x.min(MAX_SPEED);
        v.x = v.x.max(-MAX_SPEED);
        v.y = v.y.min(MAX_SPEED);
        v.y = v.y.max(-MAX_SPEED);
        v.z = v.z.min(MAX_SPEED);
        v.z = v.z.max(-MAX_SPEED);

        // apply velocity
        t.translation.x += v.x;
        t.translation.y += v.y;
        t.translation.z += v.z;

        // constrain cube to plane
        t.translation.x = t.translation.x.max(-1. * (PLANE_SIZE - CUBE_SIZE) * 0.5);
        t.translation.x = t.translation.x.min((PLANE_SIZE - CUBE_SIZE) * 0.5);
        t.translation.z = t.translation.z.max(-1. * (PLANE_SIZE - CUBE_SIZE) * 0.5);
        t.translation.z = t.translation.z.min((PLANE_SIZE - CUBE_SIZE) * 0.5);
    }
}

// all players get the same input for the synctest, handle is ignored
fn input(_handle: In<PlayerHandle>, keyboard_input: Res<Input<KeyCode>>) -> Vec<u8> {
    let mut input: u8 = 0;

    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    if keyboard_input.pressed(KeyCode::A) {
        input |= INPUT_LEFT;
    }
    if keyboard_input.pressed(KeyCode::S) {
        input |= INPUT_DOWN;
    }
    if keyboard_input.pressed(KeyCode::D) {
        input |= INPUT_RIGHT;
    }

    // if your inpute takes more than 8 bit to encode, find some way to encode to bytes
    // (for example by serializing)
    vec![input]
}

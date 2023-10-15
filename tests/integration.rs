use bevy::input::keyboard::KeyboardInput;
use bevy::input::{ButtonState, Input, InputPlugin};
use bevy::prelude::*;
use bevy::utils::HashMap;
use bevy::window::PrimaryWindow;
use bevy::MinimalPlugins;
use bevy_ggrs::{GgrsPlugin, GgrsSchedule, LocalInputs, PlayerInputs, Rollback, Session};
use bytemuck::{Pod, Zeroable};
use ggrs::{Config, P2PSession, PlayerHandle, PlayerType, SessionBuilder, UdpNonBlockingSocket};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
#[serial]
fn it_runs_advance_frame_schedule_systems() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<GgrsConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<GgrsConfig>(session2);

    let inputs1 = HashMap::from([(player1.handle, BoxInput { inp: 0 })]);
    let inputs_resource = LocalInputs::<GgrsConfig>(inputs1);
    app1.insert_resource(inputs_resource);

    // note: while this looks like it advances 50 frames, it does not
    // ggrs only advances when 16 ms has passed, so this likely only advances a single frame
    for _ in 0..50 {
        app1.update();
        app2.update();
    }

    let frame_count1 = app1.world.get_resource::<FrameCount>().unwrap();
    let frame_count2 = app2.world.get_resource::<FrameCount>().unwrap();
    assert!(frame_count1.frame > 0);
    assert!(frame_count2.frame > 0);
    assert_eq!(frame_count1.frame, frame_count2.frame);
    Ok(())
}

#[test]
#[serial]
#[ignore]
fn it_syncs_rollback_components() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<GgrsConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<GgrsConfig>(session2);

    // note: while this looks like it advances 250 frames, it does not
    // ggrs only advances when 16 ms has passed, so this likely only advances a single frame
    for _ in 0..250 {
        press_key(&mut app1, KeyCode::W);
        app1.update();
        app2.update();
    }

    let mut app2_query = app2.world.query::<(&Transform, &PlayerComponent)>();
    for (transform, player) in app2_query.iter(&app2.world) {
        if player.handle == player1.handle {
            assert!(transform.translation.z < 0., "Remote player moves forward");
        }
    }
    Ok(())
}

fn create_app<T: Config + Default>(session: P2PSession<T>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin::default())
        .add_plugins(GgrsPlugin::<T>::default())
        .insert_resource(Session::P2P(session))
        .insert_resource(FrameCount { frame: 0 })
        .add_systems(GgrsSchedule, (move_player_system, increase_frame_system))
        .add_systems(Startup, spawn_players);
    app
}

#[derive(Debug, Default)]
pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = BoxInput;
    type State = u8;
    type Address = SocketAddr;
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Pod, Zeroable)]
pub struct BoxInput {
    pub inp: u8,
}

pub struct TestPlayer {
    handle: PlayerHandle,
    address: SocketAddr,
}

fn create_players() -> (TestPlayer, TestPlayer) {
    const PLAYER1_PORT: u16 = 8081;
    const REMOTE_PORT: u16 = 8082;
    let remote_addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), PLAYER1_PORT);
    let remote_addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), REMOTE_PORT);
    return (
        TestPlayer {
            handle: 0,
            address: remote_addr1,
        },
        TestPlayer {
            handle: 1,
            address: remote_addr2,
        },
    );
}

fn start_session(
    local_player: &TestPlayer,
    remote_player: &TestPlayer,
) -> Result<P2PSession<GgrsConfig>, Box<dyn std::error::Error>> {
    let mut session_builder = SessionBuilder::<GgrsConfig>::new()
        .with_num_players(2)
        .with_max_prediction_window(12) // (optional) set max prediction window
        .with_input_delay(2); // (optional) set input delay for the local player
    session_builder = session_builder.add_player(PlayerType::Local, local_player.handle)?;
    session_builder = session_builder.add_player(
        PlayerType::Remote(remote_player.address),
        remote_player.handle,
    )?;
    let socket = UdpNonBlockingSocket::bind_to_port(local_player.address.port())?;
    let session = session_builder.start_p2p_session(socket)?;
    Ok(session)
}

const INPUT_UP: u8 = 1 << 0;

pub fn register_input_system(
    _handle: In<PlayerHandle>,
    keyboard_input: Res<Input<KeyCode>>,
) -> BoxInput {
    let mut input: u8 = 0;
    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    BoxInput { inp: input }
}

pub fn increase_frame_system(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

fn press_key(app: &mut App, key: KeyCode) {
    let window = app
        .world
        .query::<(Entity, With<PrimaryWindow>)>()
        .single(&app.world)
        .0;

    app.world.send_event(KeyboardInput {
        scan_code: 0,
        key_code: Option::from(key),
        state: ButtonState::Pressed,
        window,
    });
}

#[derive(Component, Clone, Copy, Default)]
pub struct PlayerComponent {
    pub handle: usize,
}

#[derive(Resource, Default, Reflect, Hash)]
#[reflect(Hash)]
pub struct FrameCount {
    pub frame: u32,
}

// Components that should be saved/loaded need to implement the `Reflect` trait
#[derive(Default, Reflect, Component)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub fn move_player_system(
    mut query: Query<(&mut Transform, &mut Velocity, &PlayerComponent), With<Rollback>>,
    inputs: Res<PlayerInputs<GgrsConfig>>,
) {
    const MOVEMENT_SPEED: f32 = 0.1;
    for (mut t, mut v, p) in query.iter_mut() {
        let input = inputs[p.handle].0.inp;
        if input & INPUT_UP != 0 {
            v.z -= MOVEMENT_SPEED;
        }
        t.translation.z += v.z;
    }
}

pub fn spawn_players(mut commands: Commands, session: Res<Session<GgrsConfig>>) {
    let num_players = match &*session {
        Session::SyncTest(s) => s.num_players(),
        Session::P2P(s) => s.num_players(),
        Session::Spectator(s) => s.num_players(),
    };

    for handle in 0..num_players {
        commands.spawn((
            PlayerComponent { handle },
            Velocity::default(),
            Transform::default(),
        ));
    }
}

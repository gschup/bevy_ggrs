use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use serial_test::serial;
use bevy::MinimalPlugins;
use bevy::core::{Pod, Zeroable};
use bevy::input::{ButtonState, Input, InputPlugin};
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_ggrs::{GGRSPlugin, GGRSSchedule, PlayerInputs, Rollback, RollbackIdProvider, Session};
use ggrs::{Config, SessionBuilder, PlayerType, UdpNonBlockingSocket, PlayerHandle, P2PSession};

#[test]
#[serial]
fn it_syncs_rollback_resources() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<GGRSConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<GGRSConfig>(session2);

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
fn it_syncs_rollback_components() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<GGRSConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<GGRSConfig>(session2);

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

pub struct TestPlayer {
    handle: PlayerHandle,
    address: SocketAddr,
}

fn create_players() -> (TestPlayer, TestPlayer) {
    const PLAYER1_PORT: u16 = 8081;
    const REMOTE_PORT: u16 = 8082;
    let remote_addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), PLAYER1_PORT);
    let remote_addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), REMOTE_PORT);
    return (TestPlayer { handle: 0, address: remote_addr1 }, TestPlayer { handle: 1, address: remote_addr2 });
}

fn start_session(local_player: &TestPlayer, remote_player: &TestPlayer)
                 -> Result<P2PSession<GGRSConfig>, Box<dyn std::error::Error>> {
    let mut session_builder = SessionBuilder::<GGRSConfig>::new()
        .with_num_players(2)
        .with_max_prediction_window(12) // (optional) set max prediction window
        .with_input_delay(2); // (optional) set input delay for the local player
    session_builder = session_builder.add_player(PlayerType::Local, local_player.handle)?;
    session_builder = session_builder.add_player(PlayerType::Remote(remote_player.address), remote_player.handle)?;
    let socket = UdpNonBlockingSocket::bind_to_port(local_player.address.port())?;
    let session = session_builder.start_p2p_session(socket)?;
    Ok(session)
}

fn create_app<T: Config>(session: P2PSession<T>) -> App {
    const FPS: usize = 60;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugin(InputPlugin::default());

    GGRSPlugin::<GGRSConfig>::new()
        .with_update_frequency(FPS)
        .with_input_system(rollback_input_system)
        .register_rollback_resource::<FrameCount>()
        .register_rollback_component::<Transform>()
        .register_rollback_component::<Velocity>()
        .build(&mut app);

    app.add_startup_system(spawn_players)
        .add_systems((move_player_system, increase_frame_system).in_schedule(GGRSSchedule))
        .insert_resource(Session::P2PSession(session))
        .insert_resource(FrameCount { frame: 0 });
    app
}

pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = BoxInput;
    type State = u8;
    type Address = SocketAddr;
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Pod, Zeroable)]
pub struct BoxInput {
    pub inp: u8,
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

pub fn increase_frame_system(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

const INPUT_UP: u8 = 1 << 0;

pub fn rollback_input_system(_handle: In<PlayerHandle>, keyboard_input: Res<Input<KeyCode>>) -> BoxInput {
    let mut input: u8 = 0;
    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    BoxInput { inp: input }
}


fn press_key(app: &mut App, key: KeyCode) {
    app.world.send_event(KeyboardInput {
        scan_code: 0,
        key_code: Option::from(key),
        state: ButtonState::Pressed,
    });
}

#[derive(Component, Clone, Copy, Default)]
pub struct PlayerComponent {
    pub handle: usize,
}

pub fn move_player_system(
    mut query: Query<(&mut Transform, &mut Velocity, &PlayerComponent), With<Rollback>>,
    inputs: Res<PlayerInputs<GGRSConfig>>,
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

pub fn spawn_players(
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
    session: Res<Session<GGRSConfig>>,
) {
    let num_players = match &*session {
        Session::SyncTestSession(s) => s.num_players(),
        Session::P2PSession(s) => s.num_players(),
        Session::SpectatorSession(s) => s.num_players(),
    };

    for handle in 0..num_players {
        commands.spawn((
            PlayerComponent { handle },
            Velocity::default(),
            Transform::default(),
            rip.next(),
        ));
    }
}



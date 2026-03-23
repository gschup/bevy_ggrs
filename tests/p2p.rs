use bevy::{
    MinimalPlugins,
    input::{
        ButtonInput, ButtonState, InputPlugin,
        keyboard::{Key, KeyboardInput},
    },
    platform::collections::HashMap,
    prelude::*,
    time::TimeUpdateStrategy,
};
use bevy_ggrs::{
    ConfirmedFrameCount, GgrsConfig, GgrsPlugin, GgrsResourceSnapshots, GgrsSchedule, LocalInputs,
    LocalPlayers, PlayerInputs, ReadInputs, Rollback, RollbackApp, RollbackId, Session,
};
use core::time::Duration;
use ggrs::{
    Config, P2PSession, PlayerHandle, PlayerType, SessionBuilder, SpectatorSession,
    UdpNonBlockingSocket,
};
use serde::{Deserialize, Serialize};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
#[serial]
fn it_syncs_rollback_components() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<TestConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<TestConfig>(session2);

    for _ in 0..50 {
        press_key(&mut app1, KeyCode::KeyW);
        app1.update();
        app2.update();
    }

    let mut app2_query = app2.world_mut().query::<(&Transform, &PlayerComponent)>();
    for (transform, player) in app2_query.iter(app2.world()) {
        if player.handle == player1.handle {
            assert!(transform.translation.z < 0., "Remote player moves forward");
        }
    }
    Ok(())
}

fn create_app<T: Config>(session: P2PSession<T>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin)
        .add_plugins(GgrsPlugin::<T>::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(Session::P2P(session))
        .insert_resource(FrameCount { frame: 0 })
        .add_systems(GgrsSchedule, (move_player_system, increase_frame_system))
        .add_systems(ReadInputs, read_local_inputs)
        .add_systems(Startup, spawn_players);
    app
}

type TestConfig = GgrsConfig<BoxInput>;

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    (
        TestPlayer {
            handle: 0,
            address: remote_addr1,
        },
        TestPlayer {
            handle: 1,
            address: remote_addr2,
        },
    )
}

fn start_session(
    local_player: &TestPlayer,
    remote_player: &TestPlayer,
) -> Result<P2PSession<TestConfig>, Box<dyn std::error::Error>> {
    let mut session_builder = SessionBuilder::<TestConfig>::new()
        .with_num_players(2)?
        .with_max_prediction_window(12)
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

pub fn read_local_inputs(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    local_players: Res<LocalPlayers>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input: u8 = 0;
        if keyboard_input.pressed(KeyCode::KeyW) {
            input |= INPUT_UP;
        }
        local_inputs.insert(*handle, BoxInput { inp: input });
    }

    commands.insert_resource(LocalInputs::<TestConfig>(local_inputs));
}

pub fn increase_frame_system(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut().write_message(KeyboardInput {
        logical_key: Key::Character("w".into()),
        key_code: key,
        state: ButtonState::Pressed,
        window: Entity::PLACEHOLDER,
        repeat: false,
        text: Some("w".into()),
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
    mut query: Query<(&mut Transform, &mut Velocity, &PlayerComponent), With<RollbackId>>,
    inputs: Res<PlayerInputs<TestConfig>>,
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

fn create_spectator_app(session: SpectatorSession<TestConfig>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(GgrsPlugin::<TestConfig>::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(Session::Spectator(session))
        .insert_resource(FrameCount { frame: 0 })
        .add_systems(GgrsSchedule, (move_player_system, increase_frame_system))
        .add_systems(Startup, spawn_players);
    app
}

/// Smoke test: verifies that a `SpectatorSession` runs alongside a P2P session without panicking.
///
/// Spectators receive confirmed inputs from the P2P host and advance game logic for all
/// players without providing input themselves. This test exercises the `run_spectator`
/// code path and `handle_requests` with `AdvanceFrame` requests from the spectator session.
///
/// Spectators won't advance frames if the P2P handshake hasn't completed, so only the
/// P2P frame count is asserted; the spectator is only required to not panic.
#[test]
#[serial]
fn spectator_session_does_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    const PLAYER1_PORT: u16 = 8083;
    const PLAYER2_PORT: u16 = 8084;
    const SPECTATOR_PORT: u16 = 8085;

    let player1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PLAYER1_PORT);
    let player2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PLAYER2_PORT);
    let spectator_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), SPECTATOR_PORT);

    // Player 1 is the spectator host: it registers the spectator so it will forward inputs.
    let session1 = SessionBuilder::<TestConfig>::new()
        .with_num_players(2)?
        .with_max_prediction_window(12)
        .with_input_delay(2)
        .add_player(PlayerType::Local, 0)?
        .add_player(PlayerType::Remote(player2_addr), 1)?
        .add_player(PlayerType::Spectator(spectator_addr), 2)?
        .start_p2p_session(UdpNonBlockingSocket::bind_to_port(PLAYER1_PORT)?)?;

    let session2 = SessionBuilder::<TestConfig>::new()
        .with_num_players(2)?
        .with_max_prediction_window(12)
        .with_input_delay(2)
        .add_player(PlayerType::Remote(player1_addr), 0)?
        .add_player(PlayerType::Local, 1)?
        .start_p2p_session(UdpNonBlockingSocket::bind_to_port(PLAYER2_PORT)?)?;

    let spectator_session = SessionBuilder::<TestConfig>::new()
        .with_num_players(2)?
        .start_spectator_session(
            player1_addr,
            UdpNonBlockingSocket::bind_to_port(SPECTATOR_PORT)?,
        );

    let mut app1 = create_app(session1);
    let mut app2 = create_app(session2);
    let mut spectator_app = create_spectator_app(spectator_session);

    for _ in 0..50 {
        app1.update();
        app2.update();
        spectator_app.update();
    }

    let frame_count1 = app1.world().resource::<FrameCount>().frame;
    let frame_count2 = app2.world().resource::<FrameCount>().frame;
    assert!(
        frame_count1 > 25,
        "Player 1 should advance frames (got {frame_count1})"
    );
    assert!(
        frame_count2 > 25,
        "Player 2 should advance frames (got {frame_count2})"
    );

    Ok(())
}

/// Verifies that `ConfirmedFrameCount` advances in a P2P session once frames are confirmed,
/// and that snapshots are pruned once that happens.
///
/// A dedicated `Counter` resource is registered for snapshotting. After the P2P session
/// confirms at least one frame, `ConfirmedFrameCount` must be > 0 and the frame-0
/// snapshot must have been pruned.
#[test]
#[serial]
fn p2p_confirmed_frame_advances_and_prunes_snapshots() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Resource, Clone, Default)]
    struct Counter;

    let (player1, player2) = create_players();

    let session1 = start_session(&player1, &player2)?;
    let session2 = start_session(&player2, &player1)?;

    let mut app1 = create_app::<TestConfig>(session1);
    let mut app2 = create_app::<TestConfig>(session2);

    // Register a dedicated resource for snapshotting.
    app1.init_resource::<Counter>()
        .rollback_resource_with_clone::<Counter>();

    for _ in 0..50 {
        app1.update();
        app2.update();
    }

    let confirmed = app1.world().resource::<ConfirmedFrameCount>().0;
    assert!(
        confirmed > 0,
        "ConfirmedFrameCount should advance once the P2P session confirms frames, got {confirmed}"
    );

    let snapshots = app1.world().resource::<GgrsResourceSnapshots<Counter>>();
    assert!(
        snapshots.peek(0).is_none(),
        "Frame-0 snapshot should be pruned once confirmed_frame={confirmed} > 0"
    );

    Ok(())
}

pub fn spawn_players(mut commands: Commands, session: Res<Session<TestConfig>>) {
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
            Rollback,
        ));
    }
}

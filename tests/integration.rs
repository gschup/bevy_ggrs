use bevy::{
    input::{
        keyboard::{Key, KeyboardInput},
        ButtonInput, ButtonState, InputPlugin,
    },
    prelude::*,
    time::TimeUpdateStrategy,
    utils::{Duration, HashMap},
    MinimalPlugins,
};
use bevy_ggrs::{
    AddRollbackCommandExtension, GgrsConfig, GgrsPlugin, GgrsSchedule, LocalInputs, LocalPlayers,
    PlayerInputs, ReadInputs, Rollback, Session,
};
use bytemuck::{Pod, Zeroable};
use ggrs::{Config, P2PSession, PlayerHandle, PlayerType, SessionBuilder, UdpNonBlockingSocket};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
#[serial]
fn it_runs_advance_frame_schedule_systems() -> Result<(), Box<dyn std::error::Error>> {
    let (player1, player2) = create_players();
    let session1 = start_session(&player1, &player2)?;
    let mut app1 = create_app::<TestConfig>(session1);
    let session2 = start_session(&player2, &player1)?;
    let mut app2 = create_app::<TestConfig>(session2);

    let inputs1 = HashMap::from([(player1.handle, BoxInput { inp: 0 })]);
    let inputs_resource = LocalInputs::<TestConfig>(inputs1);
    app1.insert_resource(inputs_resource);

    for _ in 0..50 {
        app1.update();
        app2.update();
    }

    let frame_count1 = app1.world().get_resource::<FrameCount>().unwrap();
    let frame_count2 = app2.world().get_resource::<FrameCount>().unwrap();

    // We've run Bevy for 50 frames, bevy_ggrs, however needs a couple of frames
    // to sync before it starts to run the advance frame schedule, so the
    // expected frame count is not 50 as one might expect.
    // We just make sure that it started running
    assert!(frame_count1.frame > 25);
    assert!(frame_count2.frame > 25);

    Ok(())
}

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
        .with_num_players(2)
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
    app.world_mut().send_event(KeyboardInput {
        logical_key: Key::Character("w".into()),
        key_code: key,
        state: ButtonState::Pressed,
        window: Entity::PLACEHOLDER,
        repeat: false,
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

pub fn spawn_players(mut commands: Commands, session: Res<Session<TestConfig>>) {
    let num_players = match &*session {
        Session::SyncTest(s) => s.num_players(),
        Session::P2P(s) => s.num_players(),
        Session::Spectator(s) => s.num_players(),
    };

    for handle in 0..num_players {
        commands
            .spawn((
                PlayerComponent { handle },
                Velocity::default(),
                Transform::default(),
            ))
            .add_rollback();
    }
}

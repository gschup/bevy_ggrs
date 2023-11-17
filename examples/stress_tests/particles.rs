use bevy::{math::vec3, prelude::*, utils::HashMap, window::WindowResolution};
use bevy_ggrs::{checksum_hasher, prelude::*, LocalInputs, LocalPlayers};
use clap::Parser;
use ggrs::{DesyncDetection, UdpNonBlockingSocket};
use rand::{Rng, SeedableRng};
use std::{
    hash::{Hash, Hasher},
    net::SocketAddr,
};

/// Stress test for bevy_ggrs
///
/// ## Basic usage:
///
/// Player 1:
///
/// cargo run --release --example particles -- --local-port 7000 --players localhost 127.0.0.1:7001
///
/// Player 2:
///
/// cargo run --release --example particles -- --local-port 7001 --players 127.0.0.1:7001 localhost
#[derive(Parser, Resource)]
struct Args {
    /// The udp port to bind to for this peer.
    #[clap(short, long)]
    local_port: u16,

    /// Address and port for the players. Order is significant. Put yourself as
    /// "localhost".
    ///
    /// e.g. `--players localhost 127.0.0.1:7001`
    #[clap(short, long, num_args = 1..)]
    players: Vec<String>,

    /// Address and port for any spectators.
    #[clap(short, long, num_args = 1..)]
    spectators: Vec<SocketAddr>,

    /// How long inputs should be kept before they are deployed. A low value,
    /// such as 0 will result in low latency, but plenty of rollbacks.
    #[clap(short, long, default_value = "2")]
    input_delay: usize,

    /// How often the clients should exchange and compare checkums of state
    #[clap(short, long, default_value = "10")]
    desync_detection_interval: u32,

    /// Whether to continue after a detected desync, the default is to panic.
    #[clap(long)]
    continue_after_desync: bool,

    /// How many particles to spawn per frame.
    #[clap(short = 'n', long, default_value = "100")]
    rate: u32,

    /// Simulation frame rate.
    #[clap(short, long, default_value = "60")]
    fps: usize,

    /// How far ahead we should simulate when we don't get any input from a player.
    #[clap(long, default_value = "8")]
    max_prediction: usize,

    /// Whether to use reflect-based rollback. This is much slower than the
    /// default clone/copy-based rollback.
    #[clap(long)]
    reflect: bool,
}

type Config = GgrsConfig<u8>;

const INPUT_SPAWN: u8 = 1 << 4;
const INPUT_NOOP: u8 = 1 << 5;

fn read_local_inputs(
    mut commands: Commands,
    keyboard_input: Res<Input<KeyCode>>,
    local_players: Res<LocalPlayers>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input: u8 = 0;

        // space triggers particles
        if keyboard_input.pressed(KeyCode::Space) {
            input |= INPUT_SPAWN;
        }

        // n is a no-op key, press to simply trigger a rollback
        if keyboard_input.pressed(KeyCode::N) {
            input |= INPUT_NOOP;
        }

        local_inputs.insert(*handle, input);
    }

    commands.insert_resource(LocalInputs::<Config>(local_inputs));
}

#[derive(Default, Reflect, Component, Clone, Copy, Deref, DerefMut)]
struct Velocity(Vec3);

impl std::hash::Hash for Velocity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We should have no NaNs or infinite values in our simulation
        // as they're not deterministic.
        assert!(
            self.0.is_finite(),
            "Hashing is not stable for NaN f32 values."
        );

        self.0.x.to_bits().hash(state);
        self.0.y.to_bits().hash(state);
        self.0.z.to_bits().hash(state);
    }
}

#[derive(Default, Reflect, Component, Clone, Copy, Deref, DerefMut)]
struct Ttl(usize);

type GameRng = rand_xoshiro::Xoshiro256PlusPlus;

#[derive(Resource, Component, Clone, Deref, DerefMut)]
struct ParticleRng(GameRng);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let num_players = args.players.len();
    assert!(num_players > 0);

    let desync_mode = match args.desync_detection_interval {
        0 => DesyncDetection::Off,
        interval => DesyncDetection::On { interval },
    };

    let mut session_builder = SessionBuilder::<Config>::new()
        .with_num_players(num_players)
        .with_desync_detection_mode(desync_mode)
        .with_max_prediction_window(args.max_prediction)?
        .with_input_delay(args.input_delay);

    for (i, player_addr) in args.players.iter().enumerate() {
        if player_addr == "localhost" {
            session_builder = session_builder.add_player(PlayerType::Local, i)?;
        } else {
            let remote_addr: SocketAddr = player_addr.parse()?;
            session_builder = session_builder.add_player(PlayerType::Remote(remote_addr), i)?;
        }
    }

    for (i, spec_addr) in args.spectators.iter().enumerate() {
        session_builder =
            session_builder.add_player(PlayerType::Spectator(*spec_addr), num_players + i)?;
    }

    let socket = UdpNonBlockingSocket::bind_to_port(args.local_port)?;
    let session = session_builder.start_p2p_session(socket)?;

    let mut app = App::new();

    app.add_plugins(GgrsPlugin::<Config>::default())
        .set_rollback_schedule_fps(args.fps)
        .add_systems(ReadInputs, read_local_inputs);

    if args.reflect {
        // SpriteBundle types
        app.rollback_component_with_reflect::<Sprite>()
            .rollback_component_with_reflect::<Transform>()
            .rollback_component_with_reflect::<GlobalTransform>()
            .rollback_component_with_reflect::<Handle<Image>>()
            .rollback_component_with_reflect::<Visibility>()
            .rollback_component_with_reflect::<InheritedVisibility>()
            .rollback_component_with_reflect::<ViewVisibility>()
            // Also add our own types
            .rollback_component_with_reflect::<Velocity>()
            .rollback_component_with_reflect::<Ttl>()
            // Xoshiro256PlusPlus doesn't implement Reflect, so have to clone
            // this is a tiny resource though, so cost of reflection would be
            // negligible anyway.
            .rollback_resource_with_clone::<ParticleRng>();
    } else {
        // clone/copy-based rollback

        // SpriteBundle types
        app.rollback_component_with_clone::<Sprite>()
            .rollback_component_with_clone::<Transform>()
            .rollback_component_with_clone::<GlobalTransform>()
            .rollback_component_with_clone::<Handle<Image>>()
            .rollback_component_with_clone::<Visibility>()
            .rollback_component_with_clone::<InheritedVisibility>()
            .rollback_component_with_clone::<ViewVisibility>()
            // Also add our own types
            .rollback_component_with_copy::<Velocity>()
            .rollback_component_with_copy::<Ttl>()
            .rollback_resource_with_clone::<ParticleRng>();
    }

    app.insert_resource(args)
        // Components can be added to the frame checksum automatically if they implement Hash...
        .checksum_component_with_hash::<Velocity>()
        // ...or you can provide a custom hashing process
        .checksum_component::<Transform>(|transform| {
            let mut hasher = checksum_hasher();

            // In this demo we only translate particles, so only that value
            // needs to be tracked.
            assert!(
                transform.translation.is_finite(),
                "Hashing is not stable for NaN f32 values."
            );

            transform.translation.x.to_bits().hash(&mut hasher);
            transform.translation.y.to_bits().hash(&mut hasher);
            transform.translation.z.to_bits().hash(&mut hasher);

            hasher.finish()
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(720., 720.),
                title: "GGRS particles stress test".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Startup, spawn_particles) // spawn an initial burst of particles
        .add_systems(
            GgrsSchedule,
            (
                spawn_particles.run_if(spawn_pressed),
                update_particles,
                despawn_particles,
            ),
        )
        .insert_resource(Session::P2P(session))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(ParticleRng(GameRng::seed_from_u64(123)))
        .add_systems(Update, print_events_system)
        .run();

    Ok(())
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn spawn_pressed(inputs: Res<PlayerInputs<Config>>) -> bool {
    inputs.iter().any(|(i, _)| *i & INPUT_SPAWN != 0)
}

fn spawn_particles(mut commands: Commands, args: Res<Args>, mut rng: ResMut<ParticleRng>) {
    let s = 200.0;
    let ttl = args.fps * 5;

    for _ in 0..args.rate {
        commands
            .spawn((
                SpriteBundle {
                    sprite: Sprite {
                        color: Color::ORANGE,
                        custom_size: Some(Vec2::splat(5.0)),
                        ..default()
                    },
                    ..default()
                },
                Velocity(vec3(rng.gen_range(-s..s), rng.gen_range(-s..s), 0.0)),
                Ttl(ttl),
            ))
            .add_rollback();
    }
}

fn update_particles(mut particles: Query<(&mut Transform, &mut Velocity)>, time: Res<Time>) {
    let time_step = time.delta_seconds();
    let gravity = Vec3::NEG_Y * 200.0;

    for (mut transform, mut velocity) in &mut particles {
        **velocity += gravity * time_step;
        transform.translation += **velocity * time_step;
    }
}

fn despawn_particles(mut commands: Commands, mut particles: Query<(Entity, &mut Ttl)>) {
    for (entity, mut ttl) in &mut particles {
        **ttl -= 1;
        if **ttl == 0 {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn print_events_system(mut session: ResMut<Session<Config>>, args: Res<Args>) {
    match session.as_mut() {
        Session::P2P(s) => {
            for event in s.events() {
                match event {
                    GgrsEvent::Disconnected { .. } | GgrsEvent::NetworkInterrupted { .. } => {
                        warn!("GGRS event: {event:?}")
                    }
                    GgrsEvent::DesyncDetected {
                        local_checksum,
                        remote_checksum,
                        frame,
                        ..
                    } => {
                        if args.continue_after_desync {
                            error!("Desync on frame {frame}. Local checksum: {local_checksum:X}, remote checksum: {remote_checksum:X}");
                        } else {
                            panic!("Desync on frame {frame}. Local checksum: {local_checksum:X}, remote checksum: {remote_checksum:X}");
                        }
                    }
                    _ => info!("GGRS event: {event:?}"),
                }
            }
        }
        _ => panic!("This example focuses on p2p."),
    }
}

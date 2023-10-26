use bevy::{math::vec3, prelude::*, utils::HashMap, window::WindowResolution};
use bevy_ggrs::{prelude::*, LocalInputs, LocalPlayers};
use clap::Parser;
use ggrs::{DesyncDetection, UdpNonBlockingSocket};
use rand::{Rng, SeedableRng};
use std::{hash::Hasher, net::SocketAddr};

#[derive(Parser, Resource)]
struct Args {
    #[clap(short, long)]
    local_port: u16,
    #[clap(short, long, num_args = 1..)]
    players: Vec<String>,
    #[clap(short, long, num_args = 1..)]
    spectators: Vec<SocketAddr>,
    #[clap(short, long, default_value = "2")]
    input_delay: usize,
    #[clap(short, long, default_value = "10")]
    desync_detection_interval: u32,
    #[clap(short = 'n', long, default_value = "100")]
    rate: u32,
    #[clap(short, long, default_value = "60")]
    fps: usize,
    #[clap(long, default_value = "8")]
    max_prediction: usize,
}

type Config = GgrsConfig<u8>;

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;

fn read_local_inputs(
    mut commands: Commands,
    keyboard_input: Res<Input<KeyCode>>,
    local_players: Res<LocalPlayers>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
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

        local_inputs.insert(*handle, input);
    }

    commands.insert_resource(LocalInputs::<Config>(local_inputs));
}

#[derive(Default, Reflect, Component, Clone, Copy, Deref, DerefMut)]
struct Velocity(Vec3);

impl std::hash::Hash for Velocity {
    fn hash<H: Hasher>(&self, state: &mut H) {
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

    App::new()
        .add_plugins(GgrsPlugin::<Config>::default())
        .set_rollback_schedule_fps(args.fps)
        .add_systems(ReadInputs, read_local_inputs)
        // SpriteBundle types
        .rollback_component_with_clone::<Sprite>()
        .rollback_component_with_clone::<Transform>()
        .rollback_component_with_clone::<GlobalTransform>()
        .rollback_component_with_clone::<Handle<Image>>()
        .rollback_component_with_clone::<Visibility>()
        .rollback_component_with_clone::<ComputedVisibility>()
        // Also add our own types
        .rollback_component_with_copy::<Velocity>()
        .rollback_component_with_copy::<Ttl>()
        .rollback_resource_with_clone::<ParticleRng>()
        .checksum_component_with_hash::<Velocity>()
        // todo: ideally we'd also be doing checksums for Transforms, but that's
        // currently very clunky to do.
        .insert_resource(args)
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
                spawn_particles.run_if(any_input),
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

fn any_input(inputs: Res<PlayerInputs<Config>>) -> bool {
    inputs.iter().any(|(i, _)| *i != 0)
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

fn update_particles(mut particles: Query<(&mut Transform, &mut Velocity)>, args: Res<Args>) {
    let time_step = 1.0 / args.fps as f32; // todo: replace with bevy_ggrs resource?
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

fn print_events_system(mut session: ResMut<Session<Config>>) {
    match session.as_mut() {
        Session::P2P(s) => {
            for event in s.events() {
                match event {
                    GgrsEvent::Disconnected { .. } | GgrsEvent::NetworkInterrupted { .. } => {
                        warn!("GGRS event: {event:?}")
                    }
                    GgrsEvent::DesyncDetected { .. } => error!("GGRS event: {event:?}"),
                    _ => info!("GGRS event: {event:?}"),
                }
            }
        }
        _ => panic!("This example focuses on p2p."),
    }
}

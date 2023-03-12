//! A simple 3D scene with light shining over a cube sitting on a plane.

use std::net::SocketAddr;

use bevy::prelude::*;
use bevy_ggrs::{AdvanceFrame, GgrsPlugin};
use bytemuck::{Pod, Zeroable};
use ggrs::Config;

/// You need to define a config struct to bundle all the generics of GGRS. You can safely ignore `State` and leave it as u8 for all GGRS functionality.
/// TODO: Find a way to hide the state type.
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

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(GgrsPlugin::<GgrsConfig>::default())
        .add_startup_system(setup)
        .add_system(yoink.in_schedule(AdvanceFrame))
        .run();
}

fn yoink() {
    println!("WHOOPS");
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // plane
    commands.spawn(PbrBundle {
        mesh: meshes.add(shape::Plane::from_size(5.0).into()),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..default()
    });
    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}

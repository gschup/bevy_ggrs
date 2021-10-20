use bevy::prelude::*;
use crate::systems::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Default, Component)]
pub struct CloudComponent {
    player_id: usize
}

impl CloudComponent {
    pub fn new(player_id: usize) -> CloudComponent {
        CloudComponent {
            player_id
        }
    }
}

pub fn cloud_system(
    mut cloud_component: Query<(&mut Transform, &CloudComponent)>,
    transform: Query<(&PlayerState)>
) {

}
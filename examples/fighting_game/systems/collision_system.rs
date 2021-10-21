use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::systems::*;
use bevy::prelude::*;
use bevy::sprite::collide_aabb::collide;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize, Component)]
pub struct ColliderSetComponent {
    pub colliders: HashMap<String, Vec<Vec<Collider>>>,
}

impl ColliderSetComponent {
    pub fn from_file(path: &Path) -> ColliderSetComponent {
        let file_contents = fs::read_to_string(path).unwrap();
        let deserialized: ColliderSetComponent = serde_json::from_str(&file_contents).unwrap();
        return deserialized;
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Eq, PartialEq, Component)]
pub enum ColliderType {
    HitBox,
    HurtBox,
}

#[derive(Copy, Clone, Serialize, Deserialize, Component)]
pub struct Collider {
    pub offset: Vec3,
    pub dimension: Vec2,
    pub collider_type: ColliderType,
}

#[allow(dead_code)]
struct ColliderEvent {
    collider_type_1: ColliderType,
    collider_type_2: ColliderType,
}

impl ColliderEvent {
    pub fn new(collider_type_1: ColliderType, collider_type_2: ColliderType) -> ColliderEvent {
        ColliderEvent {
            collider_type_1,
            collider_type_2,
        }
    }
}

pub fn collision_system(
    collider_boxes: Res<ColliderSetComponent>,
    mut player_1_query: Query<
        (
            &Transform,
            &Player1,
            &mut PlayerState,
            &mut PlayerHealth,
            &ScreenSideEnum,
        ),
        Without<Player2>,
    >,
    mut player_2_query: Query<
        (
            &Transform,
            &Player2,
            &mut PlayerState,
            &mut PlayerHealth,
            &ScreenSideEnum,
        ),
        Without<Player1>,
    >,
) {
    for (&transform_1, &_player_1, mut player_state_1, mut health_1, &player_1_side) in
        player_1_query.iter_mut()
    {
        for (&transform_2, &_player_2, mut player_state_2, mut health_2, &player_2_side) in
            player_2_query.iter_mut()
        {
            player_state_1.is_colliding = false;
            player_state_2.is_colliding = false;

            let p1_colliders = &collider_boxes.colliders[&player_state_1.player_state.to_string()]
                [player_state_1.current_sprite_index];
            let p2_colliders = &collider_boxes.colliders[&player_state_2.player_state.to_string()]
                [player_state_2.current_sprite_index];
            let mut player_1_should_inverse = 1.0f32;
            match player_1_side {
                ScreenSideEnum::Right => {
                    player_1_should_inverse = -1.0f32;
                }
                _ => {}
            }

            let mut player_2_should_inverse = 1.0f32;
            match player_2_side {
                ScreenSideEnum::Right => {
                    player_2_should_inverse = -1.0f32;
                }
                _ => {}
            }

            let mut parries = vec![];
            let mut strikes = vec![];
            let mut bounces = vec![];

            for collider_1 in p1_colliders {
                for collider_2 in p2_colliders {
                    let mut collider_1_offset = collider_1.offset.clone();
                    collider_1_offset.x = collider_1_offset.x * player_1_should_inverse;
                    let mut collider_2_offset = collider_2.offset.clone();
                    collider_2_offset.x = collider_2_offset.x * player_2_should_inverse;

                    let collision = collide(
                        transform_1.translation + collider_1_offset,
                        collider_1.dimension,
                        transform_2.translation + collider_2_offset,
                        collider_2.dimension,
                    );

                    match collision {
                        Some(_) => {
                            let collision_event = ColliderEvent::new(
                                collider_1.collider_type,
                                collider_2.collider_type,
                            );
                            if collider_1.collider_type != collider_2.collider_type {
                                strikes.push(collision_event);
                            } else if collider_1.collider_type == ColliderType::HitBox {
                                bounces.push(collision_event);
                            } else {
                                parries.push(collision_event);
                            }
                        }
                        None => {}
                    }
                }
            }

            //If we have any collision there are three possible outcomes we care about
            //1. Only hit box collisions, just means we need to handle bumping and pushing
            //2. At least 1 hurt box has hit a hit box, we need to do damage, and sent that player into the taken hit state
            //3. Two hurt boxes have hit, this is a "parry", meaning that they bounce off each other
            if parries.len() > 0 {
            } else if strikes.len() > 0 {
                let first_event = &strikes[0];
                if first_event.collider_type_1 == ColliderType::HitBox {
                    if player_state_1.can_take_a_hit() {
                        health_1.take_damage(10);
                        player_state_1.set_player_state_to_transition(PlayerStateEnum::TakeHit);
                    }
                } else if player_state_2.can_take_a_hit() {
                    if player_state_2.can_take_a_hit() {
                        health_2.take_damage(10);
                        player_state_2.set_player_state_to_transition(PlayerStateEnum::TakeHit);
                    }
                }
            } else if bounces.len() > 0 {
                player_state_1.is_colliding = true;
                player_state_2.is_colliding = true;

                match player_state_1.player_state {
                    PlayerStateEnum::Idle => {
                        player_state_1.x_velocity = player_state_2.x_velocity * 2;
                    }
                    PlayerStateEnum::Attack1 => {}
                    _ => {}
                }
                match player_state_2.player_state {
                    PlayerStateEnum::Idle => {
                        player_state_2.x_velocity = player_state_1.x_velocity * 2;
                    }
                    PlayerStateEnum::Attack1 => {}
                    _ => {}
                }
            }
        }
    }
}

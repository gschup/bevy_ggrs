use bevy::prelude::*;
use crate::systems::*;

const GRAVITY : i32 = 1;
pub const FLOOR_HEIGHT : f32 = -250.0f32;
pub const PLAYER_SPEED : i32 = 5;
pub const PLAYER_DASH_SPEED : i32 = 15;
pub const PLAYER_HIT_SPEED : i32 = 15;

pub fn player_movement_system(
    mut query: Query<(&mut Transform, &mut PlayerState)>,    
) {
    for (mut transform, mut player_state) in query.iter_mut() {
        
        transform.translation += Vec3::new(player_state.x_velocity as f32, player_state.y_velocity as f32, 0.0);

        match player_state.player_state {
            PlayerStateEnum::Run => {
            },
            PlayerStateEnum::Jump => {
                player_state.y_velocity -= GRAVITY;
                if player_state.y_velocity < 0 {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Fall);
                }
            },
            PlayerStateEnum::Fall => {
                player_state.y_velocity -= GRAVITY;
                if transform.translation.y < FLOOR_HEIGHT {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Idle);
                    player_state.y_velocity = 0;
                    transform.translation.y = FLOOR_HEIGHT;
                }
            },
            PlayerStateEnum::Idle => {
                if player_state.is_colliding == false {
                    player_state.x_velocity = 0;
                }
            },
            PlayerStateEnum::Death => {
                if transform.translation.y < FLOOR_HEIGHT {
                    player_state.y_velocity = 0;
                    transform.translation.y = FLOOR_HEIGHT;
                }
            }
            _ => {}
        }
    }
}
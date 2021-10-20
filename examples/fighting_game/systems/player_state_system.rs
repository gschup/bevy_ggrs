
use bevy::prelude::*;
use crate::systems::*;
use crate::*;

const INPUT_HISTORY_LENGTH : usize = 15;

#[derive(PartialEq, Copy, Clone, Debug, Hash, Reflect, Component)]
#[reflect(Hash)]
pub enum PlayerStateEnum {
    Idle,
    Run,
    Jump,
    Attack1,
    Fall,
    TakeHit,
    Death,
    Dash
}

impl PlayerStateEnum {
    pub fn to_string(&self) -> String {
        match self {
            PlayerStateEnum::Idle => {
                String::from("Idle")
            },
            PlayerStateEnum::Run => {
                String::from("Run")
            },
            PlayerStateEnum::Jump => {
                String::from("Jump")
            },
            PlayerStateEnum::Attack1 => {
                String::from("Attack1")
            },
            PlayerStateEnum::Fall => {
                String::from("Fall")
            },
            PlayerStateEnum::TakeHit => {
                String::from("TakeHit")
            },
            PlayerStateEnum::Death => {
                String::from("Death")
            },
            PlayerStateEnum::Dash => {
                String::from("Dash")
            }
        }
    }
}

#[derive(Default, Copy, Clone, Component)]
pub struct Player1;
#[derive(Default, Copy, Clone, Component)]
pub struct Player2;

impl Default for PlayerStateEnum {
    fn default() -> PlayerStateEnum {
        PlayerStateEnum::Idle
    }
}




#[derive(Default, Reflect, Clone, Component, Hash)]
pub struct PlayerState {
    pub player_id: usize,
    pub player_state: PlayerStateEnum,
    pub desired_player_state: PlayerStateEnum,
    pub current_sprite_index: usize,
    pub x_velocity: i32,
    pub y_velocity: i32,
    pub is_colliding: bool,
    pub state_is_dirty: bool,
    pub has_spawned_cloud: bool,
    pub inputs: Vec<InputEvents>,
    pub current_index: usize,
    pub number_of_inserted_events: usize,
    pub has_dahsed: bool
}

impl PlayerState {
    pub fn new(player_id: usize, player_state: PlayerStateEnum) -> PlayerState {
        PlayerState {
            player_id,
            player_state,
            desired_player_state: player_state,
            current_sprite_index: 0,
            x_velocity: 0,
            y_velocity: 0,
            is_colliding: false,
            state_is_dirty: true,
            has_spawned_cloud: false,
            inputs: vec![InputEvents::default(); INPUT_HISTORY_LENGTH],
            current_index: 0,
            number_of_inserted_events: 0,
            has_dahsed: false
        }
    }

    pub fn insert_input_event(&mut self, input_event: InputEvents)  {
        self.inputs[self.current_index] = input_event;
        self.current_index = (self.current_index + 1) % INPUT_HISTORY_LENGTH;
        self.number_of_inserted_events += 1;
    }

    pub fn wants_to_dash(&mut self) -> bool {
        
        return false;

        //Does the history have enough added inputs
        if self.number_of_inserted_events < INPUT_HISTORY_LENGTH {
            return false;
        }
        //Lets find out place in the ring buffer
        let start_index;
        if self.current_index == 0 {
            start_index = INPUT_HISTORY_LENGTH - 1;
        }
        else {
            start_index = self.current_index - 1;
        }

        //We only start this when a player has just struck the run keys
        if self.inputs[start_index].left_right_axis == 0 {
            return false;
        }
        //What is the value of the starting left right acis
        let dash_value = self.inputs[start_index].left_right_axis;

        //
        let mut let_go_left_right_axis = false;
        let start_index_as_isize = start_index as isize;
            
        for i in 1..INPUT_HISTORY_LENGTH {
            let mut current_back_index = start_index_as_isize - i as isize;
            if current_back_index < 0 {
                current_back_index = INPUT_HISTORY_LENGTH as isize + current_back_index;
            }
            let current_back_index = current_back_index as usize;

            if let_go_left_right_axis == false && self.inputs[current_back_index].left_right_axis != 0 {
                return false;
            }
            else if let_go_left_right_axis == false && self.inputs[current_back_index].left_right_axis == 0 {
                let_go_left_right_axis = true;
            }
            else if let_go_left_right_axis == true && self.inputs[current_back_index].left_right_axis == dash_value {
                return true;
            }
        }
        return false;
    }


    pub fn attempt_to_transition_state(&mut self) -> bool {
        let copy_of_initial_state = self.player_state.clone();
        match self.player_state {
            PlayerStateEnum::Idle => {
                self.player_state = self.desired_player_state;
            },
            PlayerStateEnum::Run => {
                self.player_state = self.desired_player_state;
            },
            PlayerStateEnum::Jump => {
            },
            PlayerStateEnum::Attack1 => {
                if self.desired_player_state == PlayerStateEnum::Jump {
                    self.player_state = self.desired_player_state;
                }
            },
            PlayerStateEnum::Fall => {
                //For now, keep it in this, but techianlly a "Landed" state would be a valid transtion for this
            },
            PlayerStateEnum::TakeHit => {

            },
            PlayerStateEnum::Death => {
                
            },
            PlayerStateEnum::Dash => {

            }
        }
        return copy_of_initial_state != self.player_state;
    } 
    
    pub fn reset_state(&mut self) {
        self.current_sprite_index = 0;
    }

    pub fn hard_reset(&mut self) {
        self.player_state = PlayerStateEnum::Idle;
        self.desired_player_state = PlayerStateEnum::Idle;
        self.current_sprite_index = 0;
        self.x_velocity = 0;
        self.y_velocity = 0;
        self.is_colliding = false;
        self.state_is_dirty = true;
        self.has_spawned_cloud = false;
    }

    pub fn animation_finished(&mut self) -> PlayerStateEnum {
        match self.player_state {
            PlayerStateEnum::Idle => {
                PlayerStateEnum::Idle
            },
            PlayerStateEnum::Run => {
                PlayerStateEnum::Run
            }
            PlayerStateEnum::Jump => {
                PlayerStateEnum::Jump
            },
            PlayerStateEnum::Attack1 => {
                PlayerStateEnum::Idle
            },
            PlayerStateEnum::Fall => {
                PlayerStateEnum::Fall
            },
            PlayerStateEnum::TakeHit => {
                PlayerStateEnum::Idle
            },
            PlayerStateEnum::Death => {
                PlayerStateEnum::Death
            },
            PlayerStateEnum::Dash => {
                PlayerStateEnum::Idle
            }
        }
    }

    pub fn can_take_a_hit(&self) -> bool {
        return self.player_state != PlayerStateEnum::TakeHit && self.desired_player_state != PlayerStateEnum::TakeHit;
    }
    
    pub fn set_player_state_to_transition(&mut self, new_player_state: PlayerStateEnum) {
        self.desired_player_state = new_player_state;
        self.state_is_dirty = true;
    }
}
pub fn player_state_system(
    mut commands: Commands,
    inputs: Res<Vec<GameInput>>,
    local_id: Res<LocalId>,
    mut query: Query<(&mut TextureAtlasSprite, Entity, &mut PlayerState, &ScreenSideEnum, &Transform)>,
    res_test: Res<TextureAtlasDictionary>
) {
    for (mut sprite, entity, mut player_state, &screen_side, &transform) in query.iter_mut() {
        let input = InputEvents::from_input_vector(&inputs, player_state.player_id);
        player_state.insert_input_event(input.clone());


        if player_state.state_is_dirty == false {
            if input.left_right_axis != 0 {
                if player_state.player_state == PlayerStateEnum::Idle {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Run);
                }
            }
            else {
                if player_state.player_state == PlayerStateEnum::Run {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Idle);
                }
            }
                    
            if input.jump_was_pressed == true {
                if player_state.player_state == PlayerStateEnum::Idle || player_state.player_state == PlayerStateEnum::Run {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Jump);
                }
            }
        
            if input.attack_1_was_pressed == true {
                if player_state.player_state == PlayerStateEnum::Idle || player_state.player_state == PlayerStateEnum::Run {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Attack1);
                }
            }
            if input.dash == true && input.left_right_axis != 0 && player_state.has_dahsed == false {
                if player_state.player_state == PlayerStateEnum::Idle || player_state.player_state == PlayerStateEnum::Run {
                    player_state.set_player_state_to_transition(PlayerStateEnum::Dash);
                    player_state.has_dahsed = true;
                }
            }
        }
        //There are a number of things we are do in the idle 
        if player_state.player_state == PlayerStateEnum::Idle {
            if input.special_ability == true && player_state.has_spawned_cloud == false {
                //Lets spawn a cloud entity at this characters feet
                player_state.has_spawned_cloud = true;
                let mut new_transform;
                if player_state.player_id != local_id.id {
                    new_transform = Transform::from_translation(Vec3::new(transform.translation.x, transform.translation.y, transform.translation.z + 1.0f32));
                }
                else {
                    new_transform = Transform::from_translation(Vec3::new(transform.translation.x, transform.translation.y, transform.translation.z - 1.0f32));
                }
                new_transform.scale.x *= 1.5f32;
                new_transform.scale.y *= 1.5f32;
                commands.spawn_bundle(SpriteBundle {
                    material: res_test.cloud_image.clone(),
                    transform: new_transform,
                    ..Default::default()
                }).insert(CloudComponent::new(player_state.player_id));
            }
        }

        if player_state.attempt_to_transition_state() || player_state.state_is_dirty {
            sprite.index = 0;
            player_state.current_sprite_index = 0;
            let next_animation;
            match player_state.desired_player_state {
                PlayerStateEnum::Idle => {
                    next_animation = "sprites/Idle.png";
                    player_state.x_velocity = 0;
                },
                PlayerStateEnum::Run => {
                    next_animation = "sprites/Run.png";
                    player_state.x_velocity = PLAYER_SPEED * input.left_right_axis as i32;
                },
                PlayerStateEnum::Jump => {
                    next_animation = "sprites/Jump.png";
                    player_state.y_velocity = 25;
                },
                PlayerStateEnum::Attack1 => {
                    next_animation = "sprites/Attack1.png";
                    player_state.x_velocity = 0;
                }
                PlayerStateEnum::Fall => {
                    next_animation = "sprites/Fall.png";
                },
                PlayerStateEnum::TakeHit => {
                    next_animation = "sprites/TakeHit.png";
                    player_state.x_velocity = PLAYER_HIT_SPEED * screen_side.back_direction() as i32;
                },
                PlayerStateEnum::Death => {
                    next_animation = "sprites/Death.png";
                    player_state.x_velocity = 0;
                },
                PlayerStateEnum::Dash => {
                    next_animation = "sprites/Dash.png";
                    player_state.x_velocity = PLAYER_DASH_SPEED * input.left_right_axis as i32;
                }
            }
            commands.entity(entity).insert(res_test.animation_handles[next_animation].clone());
            player_state.player_state = player_state.desired_player_state;
        }

        player_state.state_is_dirty = false;
    }
}
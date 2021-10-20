use bevy::prelude::*;
use crate::systems::*;

#[derive(PartialEq, Copy, Clone, Debug, Hash, Reflect, Component)]
#[reflect(Hash)]
pub enum ScreenSideEnum {
    Left,
    Right
}

impl ScreenSideEnum {
    pub fn back_direction(&self) -> f32 {
        match self {
            ScreenSideEnum::Left => {
                return -1.0f32;
            },
            ScreenSideEnum::Right => {
                return 1.0f32;
            }
        }
    }
}

impl Default for ScreenSideEnum {
    fn default() -> ScreenSideEnum {
        ScreenSideEnum::Left
    }
}

pub fn screen_side_system(
    mut commands: Commands,
    mut player_1_query: Query<(&Transform, &ScreenSideEnum, Entity, &Player1), Without<Player2>>, 
    mut player_2_query: Query<(&Transform, &ScreenSideEnum, Entity, &Player2), Without<Player1>>, 

) {
    
    for (&transform_1, &screen_side_1, entity_1, _player1) in player_1_query.iter_mut() {
        for (&transform_2, &_screen_side_2, entity_2, _player2) in player_2_query.iter_mut() {
            if transform_1.translation.x < transform_2.translation.x {
                match screen_side_1 {
                    ScreenSideEnum::Left => {},
                    ScreenSideEnum::Right => {
                        commands.entity(entity_1).insert(ScreenSideEnum::Left);
                        commands.entity(entity_2).insert(ScreenSideEnum::Right);
                    }
                }
            }
            else if transform_1.translation.x > transform_2.translation.x {
                match screen_side_1 {
                    ScreenSideEnum::Left => {
                        commands.entity(entity_1).insert(ScreenSideEnum::Right);
                        commands.entity(entity_2).insert(ScreenSideEnum::Left);
                    },
                    ScreenSideEnum::Right => {}
                }
            }
        }
    }
}
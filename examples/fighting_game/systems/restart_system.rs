use crate::systems::*;
use crate::*;

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel, Component)]
pub struct RestartSystem;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Reflect, Component)]
pub enum RestartSystemStateEnum {
    Blackout,
    OpenUp,
}

impl Default for RestartSystemStateEnum {
    fn default() -> RestartSystemStateEnum {
        return RestartSystemStateEnum::Blackout;
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Reflect, Component)]
pub struct RestartSystemState {
    system_state: RestartSystemStateEnum,
}

#[derive(Default, Copy, Clone, Component)]
pub struct UpperBlind {}

#[derive(Default, Copy, Clone, Component)]
pub struct LowerBlind {}

pub fn restart_system(
    mut commands: Commands,
    mut state: ResMut<State<GameState>>,
    mut restart_state: ResMut<RestartSystemState>,
    mut upper_blind_query: Query<
        (&UpperBlind, &mut Transform),
        (Without<LowerBlind>, Without<Player1>, Without<Player2>),
    >,
    mut lower_blind_query: Query<
        (&LowerBlind, &mut Transform),
        (Without<UpperBlind>, Without<Player1>, Without<Player2>),
    >,
    mut player_1_restart: Query<
        (
            &mut Transform,
            &mut PlayerState,
            &mut PlayerHealth,
            &mut TextureAtlasSprite,
            &Player1,
        ),
        Without<Player2>,
    >,
    mut player_2_restart: Query<
        (
            &mut Transform,
            &mut PlayerState,
            &mut PlayerHealth,
            &mut TextureAtlasSprite,
            &Player2,
        ),
        Without<Player1>,
    >,
    clouds: Query<(&CloudComponent, Entity)>,
) {
    match restart_state.system_state {
        RestartSystemStateEnum::Blackout => {
            let mut slides_in_place = 0;
            for (_up, mut transform) in lower_blind_query.iter_mut() {
                transform.translation.y += 2.0f32;
                if transform.translation.y >= -400.0f32 {
                    transform.translation.y = -400.0f32;
                    slides_in_place += 1;
                }
            }

            for (_lp, mut transform) in upper_blind_query.iter_mut() {
                transform.translation.y -= 2.0f32;
                if transform.translation.y <= 400.0f32 {
                    transform.translation.y = 400.0f32;
                    slides_in_place += 1;
                }
            }

            if slides_in_place == 2 {
                restart_state.system_state = RestartSystemStateEnum::OpenUp;
                for (mut transform, mut player_state, mut player_health, mut sprite, _player1) in
                    player_1_restart.iter_mut()
                {
                    player_state.hard_reset();
                    player_health.reset();
                    transform.translation.x = -120.0;
                    sprite.index = 0;
                }
                for (mut transform, mut player_state, mut player_health, mut sprite, _player2) in
                    player_2_restart.iter_mut()
                {
                    player_state.hard_reset();
                    player_health.reset();
                    transform.translation.x = 120.0;
                    sprite.index = 0;
                }
                for (_cloud, entity) in clouds.iter() {
                    commands.entity(entity).despawn();
                }
            }
        }
        RestartSystemStateEnum::OpenUp => {
            let mut slides_in_place = 0;
            for (_up, mut transform) in lower_blind_query.iter_mut() {
                transform.translation.y -= 2.0f32;
                if transform.translation.y <= -760.0f32 {
                    transform.translation.y = -760.0f32;
                    slides_in_place += 1;
                }
            }

            for (_lp, mut transform) in upper_blind_query.iter_mut() {
                transform.translation.y += 2.0f32;
                if transform.translation.y >= 760.0f32 {
                    transform.translation.y = 760.0f32;
                    slides_in_place += 1;
                }
            }
            if slides_in_place == 2 {
                restart_state.system_state = RestartSystemStateEnum::Blackout;
                state.set(GameState::Fighting).unwrap();
            }
        }
    }
}

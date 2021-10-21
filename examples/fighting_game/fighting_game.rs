use std::net::SocketAddr;
use std::path::Path;
use structopt::StructOpt;

use bevy::{ecs::schedule::ShouldRun, prelude::*};
use bevy_ggrs::{GGRSApp, GGRSPlugin, Rollback, RollbackIdProvider};
use ggrs::{GameInput, P2PSession, PlayerType};

use std::collections::HashMap;

mod systems;
use crate::systems::*;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Reflect, Component)]
pub enum GameState {
    Setup,
    Fighting,
    Reset,
}

impl Default for GameState {
    fn default() -> GameState {
        return GameState::Setup;
    }
}

#[derive(Default)]
pub struct TextureAtlasDictionary {
    pub animation_handles: HashMap<String, Handle<TextureAtlas>>,
    pub debug_hit_box_texture: Handle<ColorMaterial>,
    pub debug_hurt_box_texture: Handle<ColorMaterial>,
    pub cloud_image: Handle<ColorMaterial>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub struct PlayerSystem;

#[derive(Default, Component)]
pub struct CloudComponent;

const ROLLBACK_DEFAULT: &str = "rollback_default";

const FPS: u32 = 60;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    let num_players = opt.players.len();
    assert!(num_players > 0);

    let mut p2p_sess = P2PSession::new(2, INPUT_SIZE, opt.local_port)?;
    p2p_sess.set_sparse_saving(true)?;
    p2p_sess.set_fps(FPS).expect("Invalid fps");

    let collider_both = Path::new("./assets/hitboxes/character_1.json");
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(WindowDescriptor {
            title: "MKP Fighting".to_string(),
            width: 1600.,
            height: 400.,
            vsync: true,
            ..Default::default()
        })
        .add_plugin(GGRSPlugin)
        .insert_resource(opt)
        .add_state(GameState::Setup)
        .insert_resource(RestartSystemState::default())
        .insert_resource(LocalId::default())
        .insert_resource(ColliderSetComponent::from_file(&collider_both))
        .insert_resource(InputEvents::default())
        .insert_resource(TextureAtlasDictionary::default())
        .add_startup_system(start_p2p_session)
        .add_startup_system(match_setup)
        .add_startup_system(hit_box_setup_system)
        .register_rollback_type::<Transform>()
        .register_rollback_type::<PlayerState>()
        .register_rollback_type::<GameState>()
        .register_rollback_type::<Timer>()
        .with_input_system(keyboard_input_system.system())
        //Any of the systems that we wanted effected by Rollback
        //To be honest, there is some guess work in there
        .with_rollback_schedule(
            Schedule::default().with_stage(
                ROLLBACK_DEFAULT,
                SystemStage::single_threaded()
                    .with_run_criteria(game_is_fighting_state)
                    .with_system(collision_system)
                    .with_system(player_state_system)
                    .with_system(player_movement_system)
                    .with_system(sprite_system),
            ),
        )
        //Any system we don't want in rollback, but do want fun during the fighting state
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(game_is_fighting_state)
                .with_system(screen_side_system)
                .with_system(health_system_ui),
        )
        .add_system_set(
            SystemSet::new()
                .label(RestartSystem)
                .with_run_criteria(game_is_reset_state)
                .with_system(restart_system),
        )
        .with_p2p_session(p2p_sess)
        .run();
    Ok(())
}

// Only let the Fighting System set run when
// our game state is Fighthing, this is a hack to deal with some
// problems with how Bevy_ggrs handle schedules
pub fn game_is_fighting_state(state: Res<State<GameState>>) -> ShouldRun {
    match state.current() {
        GameState::Setup => ShouldRun::No,
        GameState::Fighting => ShouldRun::Yes,
        GameState::Reset => ShouldRun::No,
    }
}

// Only let the Reset System set run when
// our game state is Reset, this is a hack to deal with some
// problems with how Bevy_ggrs handle schedules
pub fn game_is_reset_state(state: Res<State<GameState>>) -> ShouldRun {
    match state.current() {
        GameState::Setup => ShouldRun::No,
        GameState::Fighting => ShouldRun::No,
        GameState::Reset => ShouldRun::Yes,
    }
}

#[derive(Copy, Clone, Component, Default, Reflect)]
pub struct SpriteTimer {
    total_frames: usize,
    current_frame: usize,
    finished: bool,
}

impl SpriteTimer {
    pub fn new(total_frames: usize) -> SpriteTimer {
        SpriteTimer {
            total_frames,
            current_frame: 0,
            finished: false,
        }
    }

    pub fn tick(&mut self) {
        self.finished = false;
        self.current_frame += 1;
        if self.current_frame == self.total_frames {
            self.finished = true;
            self.current_frame = 0;
        }
    }

    pub fn finished(&mut self) -> bool {
        self.finished
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.finished = false;
    }
}

fn sprite_system(
    texture_atlases: Res<Assets<TextureAtlas>>,
    mut query: Query<(
        &mut SpriteTimer,
        &mut TextureAtlasSprite,
        &Handle<TextureAtlas>,
        &mut PlayerState,
        &ScreenSideEnum,
    )>,
) {
    for (mut timer, mut sprite, texture_atlas_handle, mut player_state, &screen_side) in
        query.iter_mut()
    {
        //Update the timer
        timer.tick();
        let texture_atlas = texture_atlases.get(texture_atlas_handle).unwrap();
        //An odd place to do this, but ok for now, make sure the sprite is facing the right direciton
        match screen_side {
            ScreenSideEnum::Left => {
                sprite.flip_x = false;
            }
            ScreenSideEnum::Right => {
                sprite.flip_x = true;
            }
        }

        // Time to change the sprite
        if timer.finished() {
            let next = ((player_state.current_sprite_index as usize + 1)
                % texture_atlas.textures.len()) as u32;
            //As we start it at 0, we should let the system know "we have finished playing a full animation cycle, who wants next"
            if next == 0 {
                let desired_state = player_state.animation_finished();
                if desired_state == player_state.player_state {
                    player_state.reset_state();
                } else {
                    player_state.set_player_state_to_transition(desired_state);
                }
                continue;
            }
            //Make sure that the spirte, the characters idea of which sprite index we are on are in the same place
            sprite.index = next;
            player_state.current_sprite_index = next as usize;
        }
    }
}

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    players: Vec<String>,
    #[structopt(short, long)]
    spectators: Vec<SocketAddr>,
}

#[derive(Default, Component)]
pub struct LocalId {
    pub id: usize,
}

fn start_p2p_session(
    mut p2p_sess: ResMut<P2PSession>,
    opt: Res<Opt>,
    mut local_id: ResMut<LocalId>,
) {
    let mut local_handle = 0;
    let num_players = p2p_sess.num_players() as usize;

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            p2p_sess.add_player(PlayerType::Local, i).unwrap();
            local_handle = i;
            if i == 0 {
                local_id.id = 0;
            }
        } else {
            // remote players
            let remote_addr: SocketAddr =
                player_addr.parse().expect("Invalid remote player address");
            p2p_sess
                .add_player(PlayerType::Remote(remote_addr), i)
                .unwrap();
            if i == 0 {
                local_id.id = 1;
            }
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        p2p_sess
            .add_player(PlayerType::Spectator(*spec_addr), num_players + i)
            .unwrap();
    }

    // set input delay for the local player
    p2p_sess.set_frame_delay(2, local_handle).unwrap();

    // set default expected update frequency (affects synchronization timings between players)
    p2p_sess.set_fps(FPS).expect("Invalid fps");

    // start the GGRS session
    p2p_sess.start_session().unwrap();
}

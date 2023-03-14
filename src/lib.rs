//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use std::{marker::PhantomData, time::Instant};

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use bevy::utils::HashMap;
use bytemuck::Zeroable;

pub use ggrs;
use ggrs::{Config, GGRSError, GGRSRequest, InputStatus, P2PSession, PlayerHandle, SessionState, SpectatorSession, SyncTestSession};
use instant::Duration;

/// Add this component to all entities you want to be loaded/saved on rollback.
/// The `id` has to be unique. Consider using the `RollbackIdProvider` resource.
#[derive(Component)]
pub struct Rollback {
    id: u32,
}

impl Rollback {
    /// Creates a new rollback tag with the given id.
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    /// Returns the rollback id.
    pub const fn id(&self) -> u32 {
        self.id
    }
}

/// Provides unique ids for your Rollback components.
/// When you add the GGRS Plugin, this should be available as a resource.
#[derive(Resource, Default)]
pub struct RollbackIdProvider {
    next_id: u32,
}

impl RollbackIdProvider {
    /// Returns an unused, unique id.
    pub fn next_id(&mut self) -> u32 {
        if self.next_id == u32::MAX {
            // TODO: do something smart?
            panic!("RollbackIdProvider: u32::MAX has been reached.");
        }
        let ret = self.next_id;
        self.next_id += 1;
        ret
    }
}

#[derive(Resource)]
pub struct FixedTimestepData {
    /// fixed FPS for the rollback logic
    pub fps: usize,
    /// counts the number of frames that have been executed
    pub frame: i32,
    /// internal time control variables
    pub last_update: Instant,
    /// accumulated time. once enough time has been accumulated, an update is executed
    pub accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    pub run_slow: bool,
}

impl Default for FixedTimestepData {
    fn default() -> Self {
        Self {
            fps: 60,
            frame: 0,
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }
}

#[derive(Resource)]
pub struct LocalInputResource<C: Config> {
    pub inputs: HashMap<PlayerHandle, C::Input>,
}

// Label for the schedule which is advancing the gamestate by a single frame.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AdvanceFrame;

// Label for the schedule which loads and overwrites a snapshot of the world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

/// Defines the Session that the GGRS Plugin should expect as a resource.
#[derive(Resource)]
pub enum Session<T: Config> {
    SyncTestSession(SyncTestSession<T>),
    P2PSession(P2PSession<T>),
    SpectatorSession(SpectatorSession<T>),
}

/// GGRS plugin for bevy.
#[derive(Default)]
pub struct GgrsPlugin<C: Config> {
    /// fixed FPS for the rollback logic
    _marker: PhantomData<C>,
}

#[derive(Resource, Deref, DerefMut)]
pub struct PlayerInputs<T: Config>(Vec<(T::Input, InputStatus)>);

impl<C: Config> Plugin for GgrsPlugin<C> {
    fn build(&self, app: &mut App) {
        // add things to your app here
        app.add_schedule(LoadWorld, Schedule::new())
            .add_schedule(SaveWorld, Schedule::new())
            .add_schedule(AdvanceFrame, Schedule::new())
            .add_system(run_ggrs_schedules::<C>)
            .insert_resource(FixedTimestepData {
                fps: 60,
                frame: 0,
                last_update: Instant::now(),
                accumulator: Duration::ZERO,
                run_slow: false,
            });
    }
}

fn run_ggrs_schedules<C: Config>(world: &mut World, mut time_data: Local<FixedTimestepData>) {
    // no matter what, poll remotes and send responses
    if let Some(mut session) = world.get_resource_mut::<Session<C>>() {
        match &mut *session {
            Session::P2PSession(session) => {
                session.poll_remote_clients();
                time_data.run_slow = session.frames_ahead() > 0;
            }
            Session::SpectatorSession(session) => {
                session.poll_remote_clients();
            }
            _ => {}
        }
    }

    // get delta time from last run() call and accumulate it
    let delta = Instant::now().duration_since(time_data.last_update);
    let mut fps_delta = 1. / time_data.fps as f64;
    if time_data.run_slow {
        fps_delta *= 1.1;
    }
    time_data.accumulator = time_data.accumulator.saturating_add(delta);
    time_data.last_update = Instant::now();

    // if we accumulated enough time, do steps
    while time_data.accumulator.as_secs_f64() > fps_delta {
        // decrease accumulator
        time_data.accumulator = time_data
            .accumulator
            .saturating_sub(Duration::from_secs_f64(fps_delta));

        // depending on the session type, doing a single update looks a bit different
        let session = world.get_resource::<Session<C>>();
        match session {
            Some(&Session::SyncTestSession(_)) => run_synctest(world),
            Some(&Session::P2PSession(_)) => run_p2p::<C>(world),
            Some(&Session::SpectatorSession(_)) => run_spectator(world),
            _ => {
                // No session has been started yet
                time_data.last_update = Instant::now();
                time_data.accumulator = Duration::ZERO;
                time_data.frame = 0;
                time_data.run_slow = false;
            }
        }
    }

    fn run_p2p<C: Config>(world: &mut World) {
        let mut session = world.get_resource_mut::<Session<C>>();
        let Some(Session::P2PSession(ref mut sess)) = session.as_deref_mut() else {
            panic!("No GGRS P2PSession found. This should be impossible.");
        };

        // let mut local_inputs = world.get_resource::<LocalInputResource<C>>();
        // let Some(local_inputs) = local_inputs else {
        //     panic!("No LocalInputResource found.")
        // };

        if sess.current_state() == SessionState::Running {
            let local_handles = sess.local_player_handles();
            // TODO: actually get local inputs
            sess.add_local_input(local_handles[0], C::Input::zeroed())
                .expect("All handles in local handles should be valid");
            // for handle in local_handles {
            //     let input_query = local_inputs.inputs.get(&handle);
            //     if let Some(input) = input_query {
            //         sess.add_local_input(handle, *input)
            //             .expect("All handles in local_handles should be valid");
            //     }
            // }
            match sess.advance_frame() {
                Ok(requests) => handle_requests(requests, world),
                Err(GGRSError::PredictionThreshold) => {
                    info!("Skipping a frame: PredictionThreshold.")
                }
                Err(e) => println!("{}", e),
            };
        }
    }

    pub fn handle_requests<C: Config>(requests: Vec<GGRSRequest<C>>, world: &mut World) {
        for request in requests {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => world.run_schedule(SaveWorld),
                GGRSRequest::LoadGameState { frame, .. } => world.run_schedule(LoadWorld),
                GGRSRequest::AdvanceFrame { inputs } => {
                    world.insert_resource(PlayerInputs::<C>(inputs));
                    world.run_schedule(AdvanceFrame);
                }
            }
        }
    }

    fn run_spectator(world: &mut World) {}

    fn run_synctest(world: &mut World) {}
}

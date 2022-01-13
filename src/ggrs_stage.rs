use bevy::{prelude::*, reflect::TypeRegistry};
use ggrs::{
    GGRSError, GGRSRequest, GameInput, GameState, GameStateCell, P2PSession, P2PSpectatorSession,
    PlayerHandle, SessionState, SyncTestSession,
};
use instant::{Duration, Instant};

use crate::{world_snapshot::WorldSnapshot, SessionType};

/// Marker resource that triggers resetting the stage session state
pub(crate) struct GGRSStageResetSession;

/// The GGRSStage handles updating, saving and loading the game state.
pub(crate) struct GGRSStage {
    /// Inside this schedule, all rollback systems are registered.
    schedule: Schedule,
    /// Used to register all types considered when loading and saving
    pub(crate) type_registry: TypeRegistry,
    /// This system is used to get an encoded representation of the input that GGRS can handle
    pub(crate) input_system: Option<Box<dyn System<In = PlayerHandle, Out = Vec<u8>>>>,
    /// Instead of using GGRS's internal storage for encoded save states, we save the world here, avoiding serialization into `Vec<u8>`.
    snapshots: Vec<WorldSnapshot>,
    /// fixed FPS our logic is running with
    update_frequency: u32,
    /// counts the number of frames that have been executed
    frame: i32,
    /// internal time control variables
    last_update: Instant,
    /// accumulated time. once enough time has been accumulated, an update is executed
    accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    run_slow: bool,
}

impl Stage for GGRSStage {
    fn run(&mut self, world: &mut World) {
        if let Some(_) = world.remove_resource::<GGRSStageResetSession>() {
            self.reset_session();
        }

        // get delta time from last run() call and accumulate it
        let delta = Instant::now().duration_since(self.last_update);
        let mut fps_delta = 1. / self.update_frequency as f64;
        if self.run_slow {
            fps_delta *= 1.1;
        }
        self.accumulator = self.accumulator.saturating_add(delta);
        self.last_update = Instant::now();

        // no matter what, poll remotes and send responses
        if let Some(mut sess) = world.get_resource_mut::<P2PSession>() {
            sess.poll_remote_clients();
        }
        if let Some(mut sess) = world.get_resource_mut::<P2PSpectatorSession>() {
            sess.poll_remote_clients();
        }

        // if we accumulated enough time, do steps
        while self.accumulator.as_secs_f64() > fps_delta {
            // decrease accumulator
            self.accumulator = self
                .accumulator
                .saturating_sub(Duration::from_secs_f64(fps_delta));

            // depending on the session type, doing a single update looks a bit different
            let session = world.get_resource::<SessionType>();
            match session {
                Some(SessionType::SyncTestSession) => self.run_synctest(world),
                Some(SessionType::P2PSession) => self.run_p2p(world),
                Some(SessionType::P2PSpectatorSession) => self.run_spectator(world),
                None => {} // No session has been started yet
            }
        }
    }
}

impl GGRSStage {
    pub(crate) fn new() -> Self {
        Self {
            schedule: Schedule::default(),
            type_registry: TypeRegistry::default(),
            input_system: None,
            snapshots: Vec::new(),
            frame: 0,
            update_frequency: 60,
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }

    pub(crate) fn reset_session(&mut self) {
        self.last_update = Instant::now();
        self.accumulator = Duration::ZERO;
        self.frame = 0;
        self.run_slow = false;
        self.snapshots = Vec::new();
    }

    pub(crate) fn run_synctest(&mut self, world: &mut World) {
        let mut request_vec = None;

        // if our snapshot vector is not initialized, resize it accordingly
        if self.snapshots.len() == 0 {
            // find out what the maximum prediction window is in this synctest
            let max_pred = world
                .get_resource::<SyncTestSession>()
                .map(|session| session.max_prediction())
                .expect(
                "No GGRS SyncTestSession found. Please start a session and add it as a resource.",
                );
            for _ in 0..max_pred {
                self.snapshots.push(WorldSnapshot::default());
            }
        }

        // find out how many players are in this synctest
        let num_players = world
            .get_resource::<SyncTestSession>()
            .map(|session| session.num_players())
            .expect(
                "No GGRS SyncTestSession found. Please start a session and add it as a resource.",
            );

        // get inputs for all players
        let mut inputs = Vec::new();
        for handle in 0..num_players as usize {
            let input = self
                .input_system
                .as_mut()
                .expect("No input system found. Please use AppBuilder::with_input_sampler_system.")
                .run(handle, world);
            inputs.push(input);
        }

        // try to advance the frame
        match world.get_resource_mut::<SyncTestSession>() {
            Some(mut session) => match session.advance_frame(&inputs) {
                Ok(requests) => request_vec = Some(requests),
                Err(e) => println!("{}", e),
            },
            None => {
                println!("No GGRS SyncTestSession found. Please start a session and add it as a resource.")
            }
        }

        // handle all requests
        if let Some(requests) = request_vec {
            self.handle_requests(requests, world);
        }
    }

    pub(crate) fn run_spectator(&mut self, world: &mut World) {
        let mut request_vec = None;

        // run spectator session, no input necessary
        match world.get_resource_mut::<P2PSpectatorSession>() {
            Some(mut session) => {
                // if session is ready, try to advance the frame
                if session.current_state() == SessionState::Running {
                    match session.advance_frame() {
                        Ok(requests) => request_vec = Some(requests),
                        Err(GGRSError::PredictionThreshold) => {
                            println!("P2PSpectatorSession: Waiting for input from host.")
                        }
                        Err(e) => println!("{}", e),
                    };
                }

                // display all events
                for event in session.events() {
                    println!("GGRS Event: {:?}", event);
                }
            }
            None => {
                println!("No GGRS P2PSpectatorSession found. Please start a session and add it as a resource.");
            }
        }

        // handle all requests
        if let Some(requests) = request_vec {
            self.handle_requests(requests, world);
        }
    }

    pub(crate) fn run_p2p(&mut self, world: &mut World) {
        let mut request_vec = None;

        // if our snapshot vector is not initialized, resize it accordingly
        if self.snapshots.len() == 0 {
            // find out what the maximum prediction window is in this synctest
            let max_pred = world
                .get_resource::<P2PSession>()
                .map(|session| session.max_prediction())
                .expect(
                    "No GGRS P2PSession found. Please start a session and add it as a resource.",
                );
            for _ in 0..max_pred {
                self.snapshots.push(WorldSnapshot::default());
            }
        }

        // get input for the local player
        let local_handle = if let Some(session) = world.get_resource::<P2PSession>() {
            session.local_player_handle()
        } else {
            None
        }
        .expect("No GGRS P2PSession found. Please start a session and add it as a resource.");

        let input = self
            .input_system
            .as_mut()
            .expect("No input system found. Please use AppBuilder::with_input_system.")
            .run(local_handle, world);

        match world.get_resource_mut::<P2PSession>() {
            Some(mut session) => {
                // if session is ready, try to advance the frame
                if session.current_state() == SessionState::Running {
                    match session.advance_frame(local_handle, &input) {
                        Ok(requests) => request_vec = Some(requests),
                        Err(GGRSError::PredictionThreshold) => {
                            println!("Skipping a frame: PredictionThreshold.")
                        }
                        Err(e) => println!("{}", e),
                    };
                }

                // if we are ahead, run slow
                self.run_slow = session.frames_ahead() > 0;

                // display all events
                for event in session.events() {
                    println!("GGRS Event: {:?}", event);
                }
            }
            None => {
                println!(
                    "No GGRS P2PSession found. Please start a session and add it as a resource."
                );
            }
        }

        // handle all requests
        if let Some(requests) = request_vec {
            self.handle_requests(requests, world);
        }
    }

    pub(crate) fn handle_requests(&mut self, requests: Vec<GGRSRequest>, world: &mut World) {
        for request in requests {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => self.save_world(cell, frame, world),
                GGRSRequest::LoadGameState { cell, .. } => self.load_world(cell, world),
                GGRSRequest::AdvanceFrame { inputs } => self.advance_frame(inputs, world),
            }
        }
    }

    pub(crate) fn save_world(&mut self, cell: GameStateCell, frame: i32, world: &mut World) {
        assert_eq!(self.frame, frame);

        // we make a snapshot of our world
        let snapshot = WorldSnapshot::from_world(world, &self.type_registry);

        // we don't use the buffer provided by GGRS
        let state = GameState::new_with_checksum(self.frame, None, snapshot.checksum);
        cell.save(state);

        // store the snapshot ourselves (since the snapshots don't implement clone)
        let pos = frame as usize % self.snapshots.len();
        self.snapshots[pos] = snapshot;
    }

    pub(crate) fn load_world(&mut self, cell: GameStateCell, world: &mut World) {
        // since we haven't actually used the cell provided by GGRS
        let state = cell.load();
        self.frame = state.frame;

        // we get the correct snapshot
        let pos = state.frame as usize % self.snapshots.len();
        let snapshot_to_load = &self.snapshots[pos];

        // load the entities
        snapshot_to_load.write_to_world(world, &self.type_registry);
    }

    pub(crate) fn advance_frame(&mut self, inputs: Vec<GameInput>, world: &mut World) {
        world.insert_resource(inputs);
        self.schedule.run_once(world);
        world.remove_resource::<Vec<GameInput>>();
        self.frame += 1;
    }

    /*
    pub(crate) const fn update_frequency(&self) -> u32 {
        self.update_frequency
    }
    */

    pub(crate) fn set_update_frequency(&mut self, update_frequency: u32) {
        self.update_frequency = update_frequency
    }

    pub(crate) fn set_schedule(&mut self, schedule: Schedule) {
        self.schedule = schedule;
    }
}

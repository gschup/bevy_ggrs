use crate::{world_snapshot::WorldSnapshot, PlayerInputs, Session};
use bevy::{prelude::*, reflect::TypeRegistry};
use ggrs::{
    Config, GGRSError, GGRSRequest, GameStateCell, InputStatus, PlayerHandle, SessionState,
};
use instant::{Duration, Instant};

/// The GGRSStage handles updating, saving and loading the game state.
pub(crate) struct GGRSStage<T>
where
    T: Config,
{
    /// Inside this schedule, all rollback systems are registered.
    schedule: Schedule,
    /// Used to register all types considered when loading and saving
    pub(crate) type_registry: TypeRegistry,
    /// This system is used to get an encoded representation of the input that GGRS can handle
    pub(crate) input_system: Box<dyn System<In = PlayerHandle, Out = T::Input>>,
    /// Instead of using GGRS's internal storage for encoded save states, we save the world here, avoiding serialization into `Vec<u8>`.
    snapshots: Vec<WorldSnapshot>,
    /// fixed FPS our logic is running with
    update_frequency: usize,
    /// counts the number of frames that have been executed
    frame: i32,
    /// internal time control variables
    last_update: Instant,
    /// accumulated time. once enough time has been accumulated, an update is executed
    accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    run_slow: bool,
}

impl<T: Config + Send + Sync> Stage for GGRSStage<T> {
    fn run(&mut self, world: &mut World) {
        // get delta time from last run() call and accumulate it
        let delta = Instant::now().duration_since(self.last_update);
        let mut fps_delta = 1. / self.update_frequency as f64;
        if self.run_slow {
            fps_delta *= 1.1;
        }
        self.accumulator = self.accumulator.saturating_add(delta);
        self.last_update = Instant::now();

        // no matter what, poll remotes and send responses
        if let Some(mut session) = world.get_resource_mut::<Session<T>>() {
            match &mut *session {
                Session::P2PSession(session) => {
                    session.poll_remote_clients();
                }
                Session::SpectatorSession(session) => {
                    session.poll_remote_clients();
                }
                _ => {}
            }
        }

        // if we accumulated enough time, do steps
        while self.accumulator.as_secs_f64() > fps_delta {
            // decrease accumulator
            self.accumulator = self
                .accumulator
                .saturating_sub(Duration::from_secs_f64(fps_delta));

            // depending on the session type, doing a single update looks a bit different
            let session = world.get_resource::<Session<T>>();
            match session {
                Some(&Session::SyncTestSession(_)) => self.run_synctest(world),
                Some(&Session::P2PSession(_)) => self.run_p2p(world),
                Some(&Session::SpectatorSession(_)) => self.run_spectator(world),
                _ => self.reset(), // No session has been started yet
            }
        }
    }
}

impl<T: Config> GGRSStage<T> {
    pub(crate) fn new(input_system: Box<dyn System<In = PlayerHandle, Out = T::Input>>) -> Self {
        Self {
            schedule: Schedule::default(),
            type_registry: TypeRegistry::default(),
            input_system,
            snapshots: Vec::new(),
            frame: 0,
            update_frequency: 60,
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last_update = Instant::now();
        self.accumulator = Duration::ZERO;
        self.frame = 0;
        self.run_slow = false;
        self.snapshots = Vec::new();
    }

    pub(crate) fn run_synctest(&mut self, world: &mut World) {
        // let ses = world.get_resource::<Session<T>>().expect("lol");
        let Some(Session::SyncTestSession(sess)) = world.get_resource::<Session<T>>() else {
            // TODO: improve error message for new API
            panic!("No GGRS SyncTestSession found. Please start a session and add it as a resource.");
        };

        // if our snapshot vector is not initialized, resize it accordingly
        if self.snapshots.is_empty() {
            for _ in 0..sess.max_prediction() {
                self.snapshots.push(WorldSnapshot::default());
            }
        }

        // get inputs for all players
        let mut inputs = Vec::new();
        for handle in 0..sess.num_players() {
            inputs.push(self.input_system.run(handle, world));
        }

        let mut sess = world.get_resource_mut::<Session<T>>();
        let Some(Session::SyncTestSession(ref mut sess)) = sess.as_deref_mut() else {
            panic!("No GGRS SyncTestSession found. Please start a session and add it as a resource.");
        };
        for (player_handle, &input) in inputs.iter().enumerate() {
            sess.add_local_input(player_handle, input)
                .expect("All handles between 0 and num_players should be valid");
        }
        match sess.advance_frame() {
            Ok(requests) => self.handle_requests(requests, world),
            Err(e) => warn!("{}", e),
        }
    }

    pub(crate) fn run_spectator(&mut self, world: &mut World) {
        // run spectator session, no input necessary
        let mut sess = world.get_resource_mut::<Session<T>>();
        let Some(Session::SpectatorSession(ref mut sess)) = sess.as_deref_mut() else {
            // TODO: improve error message for new API
            panic!("No GGRS P2PSpectatorSession found. Please start a session and add it as a resource.");
        };

        // if session is ready, try to advance the frame
        if sess.current_state() == SessionState::Running {
            match sess.advance_frame() {
                Ok(requests) => self.handle_requests(requests, world),
                Err(GGRSError::PredictionThreshold) => {
                    info!("P2PSpectatorSession: Waiting for input from host.")
                }
                Err(e) => warn!("{}", e),
            };
        }
    }

    pub(crate) fn run_p2p(&mut self, world: &mut World) {
        let sess = world.get_resource::<Session<T>>();
        let Some(Session::P2PSession(ref sess)) = sess else {
            // TODO: improve error message for new API
            panic!("No GGRS P2PSession found. Please start a session and add it as a resource.");
        };

        // if our snapshot vector is not initialized, resize it accordingly
        if self.snapshots.is_empty() {
            // find out what the maximum prediction window is in this synctest
            for _ in 0..sess.max_prediction() {
                self.snapshots.push(WorldSnapshot::default());
            }
        }

        // if we are ahead, run slow
        self.run_slow = sess.frames_ahead() > 0;

        // get local player handles
        let local_handles = sess.local_player_handles();

        // get local player inputs
        let mut local_inputs = Vec::new();
        for &local_handle in &local_handles {
            let input = self.input_system.run(local_handle, world);
            local_inputs.push(input);
        }

        // if session is ready, try to advance the frame
        let mut sess = world.get_resource_mut::<Session<T>>();
        let Some(Session::P2PSession(ref mut sess)) = sess.as_deref_mut() else {
            // TODO: improve error message for new API
            panic!("No GGRS P2PSession found. Please start a session and add it as a resource.");
        };
        if sess.current_state() == SessionState::Running {
            for i in 0..local_inputs.len() {
                sess.add_local_input(local_handles[i], local_inputs[i])
                    .expect("All handles in local_handles should be valid");
            }
            match sess.advance_frame() {
                Ok(requests) => self.handle_requests(requests, world),
                Err(GGRSError::PredictionThreshold) => {
                    info!("Skipping a frame: PredictionThreshold.")
                }
                Err(e) => warn!("{}", e),
            };
        }
    }

    pub(crate) fn handle_requests(&mut self, requests: Vec<GGRSRequest<T>>, world: &mut World) {
        for request in requests {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => self.save_world(cell, frame, world),
                GGRSRequest::LoadGameState { frame, .. } => self.load_world(frame, world),
                GGRSRequest::AdvanceFrame { inputs } => self.advance_frame(inputs, world),
            }
        }
    }

    pub(crate) fn save_world(
        &mut self,
        cell: GameStateCell<T::State>,
        frame: i32,
        world: &mut World,
    ) {
        debug!("saving snapshot for frame {frame}");
        assert_eq!(self.frame, frame);

        // we make a snapshot of our world
        let snapshot = WorldSnapshot::from_world(world, &self.type_registry);

        // we don't really use the buffer provided by GGRS
        cell.save(self.frame, None, Some(snapshot.checksum as u128));

        // store the snapshot ourselves (since the snapshots don't implement clone)
        let pos = frame as usize % self.snapshots.len();
        self.snapshots[pos] = snapshot;
    }

    pub(crate) fn load_world(&mut self, frame: i32, world: &mut World) {
        debug!("restoring snapshot for frame {frame}");
        self.frame = frame;

        // we get the correct snapshot
        let pos = frame as usize % self.snapshots.len();
        let snapshot_to_load = &self.snapshots[pos];

        // load the entities
        snapshot_to_load.write_to_world(world, &self.type_registry);
    }

    pub(crate) fn advance_frame(
        &mut self,
        inputs: Vec<(T::Input, InputStatus)>,
        world: &mut World,
    ) {
        debug!("advancing to frame: {}", self.frame + 1);
        world.insert_resource(PlayerInputs::<T>(inputs));
        self.schedule.run_once(world);
        world.remove_resource::<PlayerInputs<T>>();
        self.frame += 1;
        debug!("frame {} completed", self.frame);
    }

    pub(crate) fn set_update_frequency(&mut self, update_frequency: usize) {
        self.update_frequency = update_frequency
    }

    pub(crate) fn set_schedule(&mut self, schedule: Schedule) {
        self.schedule = schedule;
    }

    pub(crate) fn set_type_registry(&mut self, type_registry: TypeRegistry) {
        self.type_registry = type_registry;
    }
}

use std::time::Instant;

use bevy::{ecs::schedule::ShouldRun, prelude::*, reflect::TypeRegistry};
use ggrs::{
    GGRSError, GGRSRequest, GameInput, GameState, GameStateCell, P2PSession, P2PSpectatorSession,
    PlayerHandle, SyncTestSession, MAX_PREDICTION_FRAMES,
};

use crate::{world_snapshot::WorldSnapshot, SessionType};

#[derive(Default)]
pub(crate) struct GGRSStage {
    pub(crate) session_type: SessionType,
    pub(crate) schedule: Schedule,
    pub(crate) type_registry: TypeRegistry,
    pub(crate) run_criteria: Option<Box<dyn System<In = (), Out = ShouldRun>>>,
    pub(crate) input_system: Option<Box<dyn System<In = PlayerHandle, Out = Vec<u8>>>>,
    snapshots: [WorldSnapshot; MAX_PREDICTION_FRAMES as usize + 2],
    frame: i32,
}

impl Stage for GGRSStage {
    fn run(&mut self, world: &mut World) {
        let now = Instant::now();
        // check if GGRS should run
        let should_run = match &mut self.run_criteria {
            Some(run_criteria) => run_criteria.run((), world),
            None => {
                println!("GGRSStage will not run since you didn't add any run criteria! Please use AppBuilder::with_rollback_run_criteria.");
                ShouldRun::No
            }
        };

        if should_run == ShouldRun::No {
            return;
        }

        match self.session_type {
            SessionType::SyncTestSession => self.run_synctest(world),
            SessionType::P2PSession => self.run_p2p(world),
            SessionType::P2PSpectatorSession => self.run_spectator(world),
        }

        println!("Step took {}", now.elapsed().as_micros());
    }
}

impl GGRSStage {
    fn run_synctest(&mut self, world: &mut World) {
        let num_players = if let Some(session) = world.get_resource::<SyncTestSession>() {
            Some(session.num_players())
        } else {
            None
        }
        .expect("No GGRS SyncTestSession found. Please start a session and add it as a resource.");

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
                Ok(requests) => self.handle_requests(requests, world),
                Err(e) => println!("{}", e),
            },
            None => {
                println!("No GGRS SyncTestSession found. Please start a session and add it as a resource.")
            }
        }
    }

    // run spectator session, no input necessary
    fn run_spectator(&mut self, world: &mut World) {
        match world.get_resource_mut::<P2PSpectatorSession>() {
            Some(mut session) => {
                // try to advance the frame
                match session.advance_frame() {
                    Ok(requests) => self.handle_requests(requests, world),
                    Err(GGRSError::PredictionThreshold) => {
                        println!("P2PSpectatorSession: Waiting for input from host.")
                    }
                    Err(e) => println!("{}", e),
                };
            }
            None => {
                println!("No GGRS P2PSpectatorSession found. Please start a session and add it as a resource.");
                return;
            }
        }
    }

    // run p2p session, input from local player necessary
    fn run_p2p(&mut self, world: &mut World) {
        // get input for the local player
        let local_handle = if let Some(session) = world.get_resource::<P2PSession>() {
            session.local_player_handle()
        } else {
            None
        }
        .expect("No GGRS SyncTestSession found. Please start a session and add it as a resource.");

        // get input from the local player
        let input = self
            .input_system
            .as_mut()
            .expect("No input system found. Please use AppBuilder::with_input_sampler_system.")
            .run(local_handle, world);

        match world.get_resource_mut::<P2PSession>() {
            Some(mut session) => {
                match session.advance_frame(local_handle, &input) {
                    Ok(requests) => self.handle_requests(requests, world),
                    Err(GGRSError::PredictionThreshold) => {
                        println!("Skipping a frame: PredictionThreshold.")
                    }
                    Err(e) => println!("{}", e),
                };
            }
            None => {
                println!(
                    "No GGRS P2PSession found. Please start a session and add it as a resource."
                );
                return;
            }
        }
    }

    fn handle_requests(&mut self, requests: Vec<GGRSRequest>, world: &mut World) {
        for request in requests {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => self.save_world(cell, frame, world),
                GGRSRequest::LoadGameState { cell } => self.load_world(cell, world),
                GGRSRequest::AdvanceFrame { inputs } => self.advance_frame(inputs, world),
            }
        }
    }

    fn save_world(&mut self, cell: GameStateCell, frame: i32, world: &mut World) {
        assert_eq!(self.frame, frame);

        // we don't use the buffer provided by GGRS
        let state = GameState::new(self.frame, Some(Vec::new()), Some(0));
        cell.save(state);

        // instead we make a snapshot
        let snapshot = WorldSnapshot::from_world(&world, &self.type_registry);

        // save the scene in a scene buffer
        let pos = frame as usize % self.snapshots.len();
        self.snapshots[pos] = snapshot;
    }

    fn load_world(&mut self, cell: GameStateCell, world: &mut World) {
        // since we haven't actually used the cell provided by GGRS
        let state = cell.load();
        self.frame = state.frame;

        // we get the correct scene from our own scene buffer
        let pos = state.frame as usize % self.snapshots.len();
        let snapshot_to_load = &self.snapshots[pos];

        // load the entities
        snapshot_to_load.write_to_world(world, &self.type_registry);
    }

    fn advance_frame(&mut self, inputs: Vec<GameInput>, world: &mut World) {
        world.insert_resource(inputs);
        self.schedule.run_once(world);
        world.remove_resource::<Vec<GameInput>>();
        self.frame += 1;
    }
}

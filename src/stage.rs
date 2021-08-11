use std::time::Instant;

use bevy::{ecs::schedule::ShouldRun, prelude::*, reflect::TypeRegistry};
use ggrs::{
    GGRSRequest, GameInput, GameState, GameStateCell, PlayerHandle, SyncTestSession,
    MAX_PREDICTION_FRAMES,
};

use crate::world_snapshot::WorldSnapshot;

//TODO: get rid of
const NUM_PLAYERS: u32 = 2;

#[derive(Default)]
pub(crate) struct GGRSStage {
    pub(crate) frame: i32,
    pub(crate) schedule: Schedule,
    pub(crate) type_registry: TypeRegistry,
    pub(crate) run_criteria: Option<Box<dyn System<In = (), Out = ShouldRun>>>,
    pub(crate) input_system: Option<Box<dyn System<In = PlayerHandle, Out = Vec<u8>>>>,
    pub(crate) snapshots: [WorldSnapshot; MAX_PREDICTION_FRAMES as usize + 2],
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

        // TMP get inputs for all players
        let mut inputs = Vec::new();
        for handle in 0..NUM_PLAYERS as usize {
            let input = self
                .input_system
                .as_mut()
                .expect("No input system found. Please use AppBuilder::with_input_sampler_system.")
                .run(handle, world);
            inputs.push(input);
        }

        //TODO: make this work for all kinds of sessions
        match world.get_resource_mut::<SyncTestSession>() {
            Some(mut session) => match session.advance_frame(&inputs) {
                Ok(requests) => self.handle_requests(requests, world),
                Err(e) => {
                    println!("{}", e);
                    todo!()
                }
            },
            None => {
                println!("No GGRS Session found. Please start a session and add it as a resource.")
            }
        }

        println!("Step took {}", now.elapsed().as_micros());
    }
}

impl GGRSStage {
    pub(crate) fn handle_requests(&mut self, requests: Vec<GGRSRequest>, world: &mut World) {
        for request in requests {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => self.save_world(cell, frame, world),
                GGRSRequest::LoadGameState { cell } => self.load_world(cell, world),
                GGRSRequest::AdvanceFrame { inputs } => self.advance_frame(inputs, world),
            }
        }
    }

    pub(crate) fn save_world(&mut self, cell: GameStateCell, frame: i32, world: &mut World) {
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

    pub(crate) fn load_world(&mut self, cell: GameStateCell, world: &mut World) {
        // since we haven't actually used the cell provided by GGRS
        let state = cell.load();
        self.frame = state.frame;

        // we get the correct scene from our own scene buffer
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
}

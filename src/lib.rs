//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

pub use ggrs;

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

// A label for our new Schedule!
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AdvanceFrame;

// A label for our new Schedule!
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

// A label for our new Schedule!
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

/// A builder to configure GGRS for a bevy app.
pub struct GgrsPlugin;

impl Plugin for GgrsPlugin {
    fn build(&self, app: &mut App) {
        // add things to your app here
        app.add_schedule(LoadWorld, Schedule::new())
            .add_schedule(SaveWorld, Schedule::new())
            .add_schedule(AdvanceFrame, Schedule::new())
            .add_system(run_ggrs_schedules.in_schedule(CoreSchedule::FixedUpdate));
    }
}

fn run_ggrs_schedules(world: &mut World) {
    world.run_schedule(LoadWorld);
    world.run_schedule(AdvanceFrame);
    world.run_schedule(SaveWorld);
}

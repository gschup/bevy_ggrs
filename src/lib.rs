//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
    reflect::{FromType, GetTypeRegistration, TypeRegistry, TypeRegistryInternal},
    utils::HashMap,
};
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use instant::{Duration, Instant};
use parking_lot::RwLock;
use std::{marker::PhantomData, sync::Arc};
use world_snapshot::RollbackSnapshots;

pub use ggrs;

pub use rollback::{AddRollbackCommand, AddRollbackCommandExtension, Rollback};

pub(crate) mod ggrs_stage;
pub(crate) mod rollback;
pub(crate) mod world_snapshot;

pub mod prelude {
    pub use crate::{
        AddRollbackCommandExtension, GgrsApp, GgrsPlugin, GgrsSchedule, PlayerInputs, ReadInputs,
        Rollback, Session,
    };
    pub use ggrs::{GGRSEvent as GgrsEvent, PlayerType, SessionBuilder};
}

const DEFAULT_FPS: usize = 60;

#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct GgrsSchedule;

/// Defines the Session that the GGRS Plugin should expect as a resource.
#[derive(Resource)]
pub enum Session<T: Config> {
    SyncTest(SyncTestSession<T>),
    P2P(P2PSession<T>),
    Spectator(SpectatorSession<T>),
}

// TODO: more specific name to avoid conflicts?
#[derive(Resource, Deref, DerefMut)]
pub struct PlayerInputs<T: Config>(Vec<(T::Input, InputStatus)>);

#[derive(Resource, Copy, Clone, Debug)]
struct FixedTimestepData {
    /// fixed FPS our logic is running with
    pub fps: usize,
    /// internal time control variables
    last_update: Instant,
    /// accumulated time. once enough time has been accumulated, an update is executed
    accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    run_slow: bool,
}

impl Default for FixedTimestepData {
    fn default() -> Self {
        Self {
            fps: DEFAULT_FPS,
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }
}

/// Keeps track of the current frame the rollback simulation is in
#[derive(Resource, Debug, Default)]
pub struct RollbackFrameCount(i32);

#[derive(Resource)]
struct RollbackTypeRegistry(TypeRegistry);

/// Inputs from local players. You have to fill this resource in the ReadInputs schedule.
#[derive(Resource)]
pub struct LocalInputs<C: Config>(pub HashMap<PlayerHandle, C::Input>);

/// Handles for the local players, you can use this when writing an input system.
#[derive(Resource, Default)]
pub struct LocalPlayers(pub Vec<PlayerHandle>);

impl Default for RollbackTypeRegistry {
    fn default() -> Self {
        Self(TypeRegistry {
            internal: Arc::new(RwLock::new({
                let mut r = TypeRegistryInternal::empty();
                // `Parent` and `Children` must be registered so that their `ReflectMapEntities`
                // data may be used.
                //
                // While this is a little bit of a weird spot to register these, are the only
                // Bevy core types implementing `MapEntities`, so for now it's probably fine to
                // just manually register these here.
                //
                // The user can still register any custom types with `register_rollback_type()`.
                r.register::<Parent>();
                r.register::<Children>();
                r
            })),
        })
    }
}

impl RollbackTypeRegistry {
    /// Registers a type of component for saving and loading during rollbacks.
    pub fn register_rollback_component<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        let mut registry = self.0.write();
        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectComponent as FromType<Type>>::from_type());
        drop(registry);
        self
    }

    /// Registers a type of resource for saving and loading during rollbacks.
    pub fn register_rollback_resource<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Resource,
    {
        let mut registry = self.0.write();
        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectResource as FromType<Type>>::from_type());
        drop(registry);
        self
    }
}

// Label for the schedule which reads the inputs for the current frame
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReadInputs;

/// GGRS plugin for bevy.
pub struct GgrsPlugin<C: Config> {
    /// phantom marker for ggrs config
    _marker: PhantomData<C>,
}

impl<C: Config> Default for GgrsPlugin<C> {
    fn default() -> Self {
        Self {
            _marker: Default::default(),
        }
    }
}

impl<C: Config> Plugin for GgrsPlugin<C> {
    fn build(&self, app: &mut App) {
        let mut schedule = Schedule::default();
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..default()
        });

        app.init_resource::<RollbackTypeRegistry>()
            .init_resource::<RollbackSnapshots>()
            .init_resource::<RollbackFrameCount>()
            .init_resource::<LocalPlayers>()
            .add_schedule(GgrsSchedule, schedule)
            .add_schedule(ReadInputs, Schedule::new())
            .add_systems(PreUpdate, ggrs_stage::run::<C>);
    }
}

/// Extension trait to add the GGRS plugin idiomatically to Bevy Apps
pub trait GgrsApp {
    /// Registers a component type for saving and loading from the world.
    fn register_rollback_component<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component;

    /// Registers a resource type for saving and loading from the world.
    fn register_rollback_resource<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Resource;

    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self;
}

impl GgrsApp for App {
    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self {
        let mut time_data = FixedTimestepData::default();
        time_data.fps = fps;
        self.world.insert_resource(time_data);
        self
    }

    fn register_rollback_component<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        self.world
            .get_resource_mut::<RollbackTypeRegistry>()
            .expect("RollbackTypeRegistry not found. Did you add the GgrsPlugin?")
            .register_rollback_component::<Type>();
        self
    }

    fn register_rollback_resource<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Resource,
    {
        self.world
            .get_resource_mut::<RollbackTypeRegistry>()
            .expect("RollbackTypeRegistry not found. Did you add the GgrsPlugin?")
            .register_rollback_resource::<Type>();
        self
    }
}

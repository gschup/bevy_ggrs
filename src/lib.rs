//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
    reflect::{FromType, GetTypeRegistration, TypeRegistryInternal},
    utils::{Duration, HashMap},
};
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use schedule_systems::{load_world, save_world};
use std::{fmt::Debug, hash::Hash, marker::PhantomData, net::SocketAddr};
use world_snapshot::RollbackSnapshots;

pub use ggrs;

pub use rollback::{AddRollbackCommand, AddRollbackCommandExtension, Rollback};

pub(crate) mod rollback;
pub(crate) mod schedule_systems;
pub(crate) mod world_snapshot;

pub mod prelude {
    pub use crate::{
        AddRollbackCommandExtension, GgrsApp, GgrsConfig, GgrsPlugin, GgrsSchedule, PlayerInputs,
        ReadInputs, Rollback, Session,
    };
    pub use ggrs::{GGRSEvent as GgrsEvent, PlayerType, SessionBuilder};
}

/// A sensible default [GGRS Config](`ggrs::Config`) type suitable for most applications.
///
/// If you require a more specialized configuration, you can create your own type implementing
/// [`Config`](`ggrs::Config`).
#[derive(Debug)]
pub struct GgrsConfig<Input, Address = SocketAddr, State = u8> {
    _phantom: PhantomData<(Input, Address, State)>,
}

impl<Input, Address, State> Config for GgrsConfig<Input, Address, State>
where
    Self: 'static,
    Input: Send + Sync + PartialEq + bytemuck::Pod,
    Address: Send + Sync + Debug + Hash + Eq + Clone,
    State: Send + Sync + Clone,
{
    type Input = Input;
    type State = State;
    type Address = Address;
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
    /// accumulated time. once enough time has been accumulated, an update is executed
    accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    run_slow: bool,
}

impl Default for FixedTimestepData {
    fn default() -> Self {
        Self {
            fps: DEFAULT_FPS,
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }
}

/// Keeps track of the current frame the rollback simulation is in
#[derive(Resource, Debug, Default)]
pub struct RollbackFrameCount(i32);

#[derive(Resource)]
struct RollbackTypeRegistry(TypeRegistryInternal);

/// Inputs from local players. You have to fill this resource in the ReadInputs schedule.
#[derive(Resource)]
pub struct LocalInputs<C: Config>(pub HashMap<PlayerHandle, C::Input>);

/// Handles for the local players, you can use this when writing an input system.
#[derive(Resource, Default)]
pub struct LocalPlayers(pub Vec<PlayerHandle>);

impl Default for RollbackTypeRegistry {
    fn default() -> Self {
        Self({
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
        })
    }
}

impl RollbackTypeRegistry {
    /// Registers a type of component for saving and loading during rollbacks.
    pub fn register_rollback_component<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        let registry = &mut self.0;
        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectComponent as FromType<Type>>::from_type());
        self
    }

    /// Registers a type of resource for saving and loading during rollbacks.
    pub fn register_rollback_resource<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Resource,
    {
        let registry = &mut self.0;
        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectResource as FromType<Type>>::from_type());
        self
    }
}

/// Label for the schedule which reads the inputs for the current frame
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReadInputs;

/// Label for the schedule which loads and overwrites a snapshot of the world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

/// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

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
            .init_resource::<FixedTimestepData>()
            .add_schedule(GgrsSchedule, schedule)
            .add_schedule(ReadInputs, Schedule::new())
            .add_systems(PreUpdate, schedule_systems::run_ggrs_schedules::<C>)
            .add_systems(LoadWorld, load_world)
            .add_systems(SaveWorld, save_world);
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

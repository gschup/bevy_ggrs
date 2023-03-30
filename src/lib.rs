//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy::reflect::{FromType, GetTypeRegistration, TypeRegistry, TypeRegistryInternal};
use bevy::utils::HashMap;
use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use instant::{Duration, Instant};
use parking_lot::RwLock;
use std::{marker::PhantomData, sync::Arc};

pub use ggrs;
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};

pub(crate) mod schedule_systems;
pub(crate) mod world_snapshot;
use schedule_systems::run_ggrs_schedules;

pub const DEFAULT_FPS: usize = 60;

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

    /// Returns a `Rollback` component with the next unused id
    ///
    /// Convenience for `Rollback::new(rollback_id_provider.next_id())`.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_ggrs::{RollbackIdProvider};
    ///
    /// fn system_in_rollback_schedule(mut commands: Commands, mut rip: RollbackIdProvider) {
    ///     commands.spawn((
    ///         SpatialBundle::default(),
    ///         rip.next(),
    ///     ));
    /// }
    /// ```
    pub fn next(&mut self) -> Rollback {
        Rollback::new(self.next_id())
    }
}

#[derive(Resource, Copy, Clone)]
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
            fps: DEFAULT_FPS,
            frame: 0,
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }
}

/// Inputs from local players. You have to fill this resource in the ReadInputs schedule.
#[derive(Resource)]
pub struct LocalInputs<C: Config>(HashMap<PlayerHandle, C::Input>);

/// Inputs for all players. Will be inserted by the ggrs plugin before every execution of the AdvanceFrame schedule.
#[derive(Resource, Deref, DerefMut)]
pub struct SynchronizedInputs<C: Config>(Vec<(C::Input, InputStatus)>);

/// Defines the Session that the GGRS Plugin should expect as a resource.
#[derive(Resource)]
pub enum Session<T: Config> {
    SyncTestSession(SyncTestSession<T>),
    P2PSession(P2PSession<T>),
    SpectatorSession(SpectatorSession<T>),
}

#[derive(Resource)]
pub struct RollbackTypeRegistry(TypeRegistry);

impl RollbackTypeRegistry {
    pub fn default() -> Self {
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

// Label for the schedule which is advancing the gamestate by a single frame.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AdvanceFrame;

// Label for the schedule which loads and overwrites a snapshot of the world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReadInputs;

#[derive(Default)]
/// GGRS plugin for bevy.
pub struct GgrsPlugin<C: Config> {
    /// phantom marker for ggrs config
    _marker: PhantomData<C>,
}

impl<C: Config> Plugin for GgrsPlugin<C> {
    fn build(&self, app: &mut App) {
        // configure AdvanceFrame schedule
        let mut schedule = Schedule::default();
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..default()
        });
        // add everything to the app
        app.add_schedule(LoadWorld, Schedule::new())
            .add_schedule(SaveWorld, Schedule::new())
            .add_schedule(AdvanceFrame, schedule)
            .add_schedule(ReadInputs, Schedule::new())
            .add_system(run_ggrs_schedules::<C>)
            .insert_resource(FixedTimestepData::default())
            .insert_resource(RollbackIdProvider::default())
            .insert_resource(RollbackTypeRegistry::default());
    }
}

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

    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self {
        let mut time_data = FixedTimestepData::default();
        time_data.fps = fps;
        self.world.insert_resource(time_data);
        self
    }
}

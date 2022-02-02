//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::system::Command,
    prelude::*,
    reflect::{FromType, GetTypeRegistration, TypeRegistry},
};
use ggrs::{Config, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use ggrs_stage::{GGRSStage, GGRSStageResetSession};
use reflect_resource::ReflectResource;
use std::marker::PhantomData;

pub(crate) mod ggrs_stage;
pub(crate) mod reflect_resource;
pub(crate) mod world_snapshot;

/// Stage label for the Custom GGRS Stage.
pub const GGRS_UPDATE: &str = "ggrs_update";
const DEFAULT_FPS: u32 = 60;

/// Defines the Session that the GGRS Plugin should expect as a resource.
/// Use `with_session_type(type)` to set accordingly.
pub enum SessionType {
    SyncTestSession,
    P2PSession,
    SpectatorSession,
}

impl Default for SessionType {
    fn default() -> Self {
        SessionType::SyncTestSession
    }
}

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
#[derive(Default)]
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

/// This registry is used to save all types that will be rolled back with GGRS.
#[derive(Default)]
pub struct RollbackTypeRegistry {
    pub registry: TypeRegistry,
}

/// A builder to configure GGRS for a bevy app.
pub struct GGRSPlugin<T: Config + Send + Sync> {
    input_system: Option<Box<dyn System<In = PlayerHandle, Out = T::Input>>>,
    fps: u32,
    type_registry: RollbackTypeRegistry,
    schedule: Schedule,
}

impl<T: Config + Send + Sync> GGRSPlugin<T> {
    /// Create a new instance of the builder.
    pub fn new() -> Self {
        Self {
            input_system: None,
            fps: DEFAULT_FPS,
            type_registry: RollbackTypeRegistry::default(),
            schedule: Default::default(),
        }
    }

    /// Change the update frequency of the rollback stage.
    pub fn with_update_frequency(mut self, fps: u32) -> Self {
        self.fps = fps;
        self
    }

    /// Registers a system that takes player handles as input and returns the associated inputs for that player.
    pub fn with_input_system<Params>(
        mut self,
        input_fn: impl IntoSystem<PlayerHandle, T::Input, Params>,
    ) -> Self {
        self.input_system = Some(Box::new(input_fn.system()));
        self
    }

    /// Registers a type of component for saving and loading during rollbacks.
    pub fn register_rollback_type<Type>(self) -> Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        let mut registry = self.type_registry.registry.write();

        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectComponent as FromType<Type>>::from_type());
        registration.insert(<ReflectResource as FromType<Type>>::from_type());
        drop(registry);
        self
    }

    /// Adds a schedule into the GGRSStage that holds the game logic systems. This schedule should contain all
    /// systems you want to be executed during frame advances.
    pub fn with_rollback_schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// Consumes the builder and makes changes on the bevy app according to the settings.
    pub fn build(self, app: &mut App) {
        let mut input_system = self
            .input_system
            .expect("Adding an input system through GGRSBuilder::with_input_system is required");
        // ggrs stage
        input_system.initialize(&mut app.world);
        let mut stage = GGRSStage::<T>::new(input_system);
        stage.set_update_frequency(self.fps);
        stage.set_schedule(self.schedule);
        app.add_stage_before(CoreStage::Update, GGRS_UPDATE, stage);
        // insert a rollback id provider
        app.insert_resource(RollbackIdProvider::default());
        // insert rollback type registry
        app.insert_resource(self.type_registry);
    }
}

pub trait CommandsExt<T>
where
    T: Config,
{
    fn stop_session(&mut self);
}

impl<T: Config + Send + Sync> CommandsExt<T> for Commands<'_, '_> {
    fn stop_session(&mut self) {
        self.add(StopSessionCommand::<T>::new());
    }
}

#[derive(Default)]
struct StopSessionCommand<T>
where
    T: Config + Send + Sync,
{
    phantom_data: PhantomData<T>,
}

impl<T: Config + Send + Sync> StopSessionCommand<T> {
    pub(crate) fn new() -> Self {
        Self {
            phantom_data: PhantomData::<T>::default(),
        }
    }
}

impl<T: Config + Send + Sync> Command for StopSessionCommand<T> {
    fn write(self, world: &mut World) {
        world.remove_resource::<SessionType>();
        world.remove_resource::<P2PSession<T>>();
        world.remove_resource::<SyncTestSession<T>>();
        world.remove_resource::<SpectatorSession<T>>();
        world.insert_resource(GGRSStageResetSession);
    }
}

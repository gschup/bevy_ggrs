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

#[derive(Default)]
pub struct RollbackTypeRegistry {
    pub registry: TypeRegistry,
}

/// Provides all functionality for the GGRS p2p rollback networking library.
pub struct GGRSPlugin<T>
where
    T: Config,
{
    phantom_data: PhantomData<T>,
}

impl<T: Config> Default for GGRSPlugin<T> {
    fn default() -> Self {
        Self {
            phantom_data: Default::default(),
        }
    }
}

impl<T: Config + Send + Sync> Plugin for GGRSPlugin<T> {
    fn build(&self, app: &mut App) {
        // ggrs stage
        app.add_stage_before(CoreStage::Update, GGRS_UPDATE, GGRSStage::<T>::new());
        // insert a rollback id provider
        app.insert_resource(RollbackIdProvider::default());
        // insert rollback type registry
        app.insert_resource(RollbackTypeRegistry::default());
    }
}

/// Extension trait for the `App`.
pub trait GGRSApp {
    /// Adds the given `ggrs::SyncTestSession` to your app.
    fn with_synctest_session<T: Config>(&mut self, sess: SyncTestSession<T>) -> &mut Self;

    /// Adds the given `ggrs::P2PSession` to your app.
    fn with_p2p_session<T: Config>(&mut self, sess: P2PSession<T>) -> &mut Self;

    /// Adds the given `ggrs::P2PSpectatorSession` to your app.
    fn with_p2p_spectator_session<T: Config>(&mut self, sess: SpectatorSession<T>) -> &mut Self;

    /// Adds a schedule into the GGRSStage that holds the game logic systems. This schedule should contain all
    /// systems you want to be executed during frame advances.
    fn with_rollback_schedule<T: Config>(&mut self, schedule: Schedule) -> &mut Self;

    /// Registers a given system as the input system. This system should provide encoded inputs for a given player.
    fn with_input_system<T, S>(&mut self, input_system: S) -> &mut Self
    where
        T: Config,
        S: System<In = PlayerHandle, Out = T::Input> + Send + Sync + 'static;

    /// Sets the fixed update frequency
    fn with_update_frequency<T: Config>(&mut self, update_frequency: u32) -> &mut Self;

    /// Registers a type of component for saving and loading during rollbacks.
    fn register_rollback_type<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component;
}

impl GGRSApp for App {
    fn with_synctest_session<T: Config>(&mut self, session: SyncTestSession<T>) -> &mut Self {
        self.insert_resource(SessionType::SyncTestSession);
        self.insert_resource(session);
        self
    }

    fn with_p2p_session<T: Config>(&mut self, session: P2PSession<T>) -> &mut Self {
        self.insert_resource(SessionType::P2PSession);
        self.insert_resource(session);
        self
    }

    fn with_p2p_spectator_session<T: Config>(&mut self, session: SpectatorSession<T>) -> &mut Self {
        self.insert_resource(SessionType::SpectatorSession);
        self.insert_resource(session);
        self
    }

    fn with_rollback_schedule<T: Config>(&mut self, schedule: Schedule) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage<T>>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.set_schedule(schedule);
        self
    }

    fn with_input_system<T, S>(&mut self, input_system: S) -> &mut Self
    where
        T: Config,
        S: System<In = PlayerHandle, Out = T::Input> + Send + Sync + 'static,
    {
        let mut input_system = input_system.system();
        input_system.initialize(&mut self.world);
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage<T>>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.input_system = Some(Box::new(input_system));
        self
    }

    fn with_update_frequency<T: Config>(&mut self, update_frequency: u32) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage<T>>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.set_update_frequency(update_frequency);
        self
    }

    fn register_rollback_type<Type>(&mut self) -> &mut Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        let rollback_registry = self
            .world
            .get_resource_mut::<RollbackTypeRegistry>()
            .expect("No RollbackTypeRegistry found! Did you install the GGRSPlugin?");
        let mut registry = rollback_registry.registry.write();

        registry.register::<Type>();

        let registration = registry.get_mut(std::any::TypeId::of::<Type>()).unwrap();
        registration.insert(<ReflectComponent as FromType<Type>>::from_type());
        registration.insert(<ReflectResource as FromType<Type>>::from_type());
        drop(registry);

        self
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
    T: Config,
{
    phantom_data: PhantomData<T>,
}

impl<T: Config> StopSessionCommand<T> {
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

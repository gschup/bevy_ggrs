//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::entity::EntityMap,
    prelude::*,
    reflect::{FromType, GetTypeRegistration, TypeRegistry, TypeRegistryInternal},
};
use ggrs::{Config, Frame, PlayerHandle};
use ggrs_stage::GGRSStage;
use parking_lot::RwLock;
use reflect_resource::ReflectResource;
use std::sync::Arc;

pub use ggrs;

pub(crate) mod ggrs_stage;
pub(crate) mod reflect_resource;
pub(crate) mod world_snapshot;

/// Stage label for the Custom GGRS Stage.
pub const GGRS_UPDATE: &str = "ggrs_update";
const DEFAULT_FPS: usize = 60;

/// Defines the Session that the GGRS Plugin should expect as a resource.
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

/// An object that may hook into rollback events.
pub trait RollbackEventHook: Sync + Send + 'static {
    /// Run before a snapshot is saved
    fn pre_save(&mut self, frame: Frame, max_snapshots: usize, world: &mut World) {
        let _ = (frame, max_snapshots, world);
    }
    /// Run after a snapshot is saved
    fn post_save(&mut self, frame: Frame, max_snapshots: usize, world: &mut World) {
        let _ = (frame, max_snapshots, world);
    }
    /// Run before a snapshot is loaded ( restored )
    fn pre_load(&mut self, frame: Frame, max_snapshots: usize, world: &mut World) {
        let _ = (frame, max_snapshots, world);
    }
    /// Run after a snapshot is loaded ( restored )
    fn post_load(
        &mut self,
        frame: Frame,
        max_snapshots: usize,
        entity_map: &EntityMap,
        world: &mut World,
    ) {
        let _ = (frame, max_snapshots, entity_map, world);
    }
    /// Run before the game simulation is advanced one frame
    fn pre_advance(&mut self, world: &mut World) {
        let _ = world;
    }
    /// Run after the game simulation is advanced one frame
    fn post_advance(&mut self, world: &mut World) {
        let _ = world;
    }
}

/// A builder to configure GGRS for a bevy app.
pub struct GGRSPlugin<T: Config + Send + Sync> {
    input_system: Option<Box<dyn System<In = PlayerHandle, Out = T::Input>>>,
    fps: usize,
    type_registry: TypeRegistry,
    schedule: Schedule,
    hooks: Vec<Box<dyn RollbackEventHook>>,
}

impl<T: Config + Send + Sync> Default for GGRSPlugin<T> {
    fn default() -> Self {
        Self {
            input_system: None,
            fps: DEFAULT_FPS,
            type_registry: TypeRegistry {
                internal: Arc::new(RwLock::new({
                    let mut r = TypeRegistryInternal::empty();
                    // `Parent` and `Children` must be regisrered so that their `ReflectMapEntities`
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
            },
            schedule: Default::default(),
            hooks: Default::default(),
        }
    }
}

impl<T: Config + Send + Sync> GGRSPlugin<T> {
    /// Create a new instance of the builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Change the update frequency of the rollback stage.
    pub fn with_update_frequency(mut self, fps: usize) -> Self {
        self.fps = fps;
        self
    }

    /// Registers a system that takes player handles as input and returns the associated inputs for that player.
    pub fn with_input_system<Params>(
        mut self,
        input_fn: impl IntoSystem<PlayerHandle, T::Input, Params>,
    ) -> Self {
        self.input_system = Some(Box::new(IntoSystem::into_system(input_fn)));
        self
    }

    /// Registers a type of component for saving and loading during rollbacks.
    pub fn register_rollback_type<Type>(self) -> Self
    where
        Type: GetTypeRegistration + Reflect + Default + Component,
    {
        let mut registry = self.type_registry.write();
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

    /// Add a [`RollbackEventHook`] object will have the opportunity to modify the world or take
    /// other actions during snapshot saves, loads, and frame advances.q
    pub fn add_rollback_hook<H: RollbackEventHook>(mut self, hook: H) -> Self {
        self.hooks.push(Box::new(hook));
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
        stage.set_type_registry(self.type_registry);
        stage.set_hooks(self.hooks);
        app.add_stage_before(CoreStage::Update, GGRS_UPDATE, stage);
        // other resources
        app.insert_resource(RollbackIdProvider::default());
    }
}

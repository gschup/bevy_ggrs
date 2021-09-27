//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::{
        schedule::{IntoSystemDescriptor, Stage},
        system::Command,
    },
    prelude::*,
    reflect::{FromType, GetTypeRegistration},
};
use ggrs::{P2PSession, P2PSpectatorSession, PlayerHandle, SessionState, SyncTestSession};
use ggrs_stage::GGRSStage;
use reflect_resource::ReflectResource;

pub(crate) mod ggrs_stage;
pub(crate) mod reflect_resource;
pub(crate) mod world_snapshot;

/// Stage label for the Custom GGRS Stage.
pub const GGRS_UPDATE: &str = "ggrs_update";
/// Stage label for the default internal GGRS System Stage, where all rollback systems will be added to by default.
pub const ROLLBACK_DEFAULT: &str = "rollback_default";

/// Defines the Session that the GGRS Plugin should expect as a resource.
/// Use `with_session_type(type)` to set accordingly.
pub enum SessionType {
    SyncTestSession,
    P2PSession,
    P2PSpectatorSession,
}

impl Default for SessionType {
    fn default() -> Self {
        SessionType::SyncTestSession
    }
}

/// Add this component to all entities you want to be loaded/saved on rollback.
/// The `id` has to be unique. Consider using the `RollbackIdProvider` resource.
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

/// Provides all functionality for the GGRS p2p rollback networking library.
pub struct GGRSPlugin;

impl Plugin for GGRSPlugin {
    fn build(&self, app: &mut App) {
        // everything for the GGRS stage, where all rollback systems will be executed
        let mut schedule = Schedule::default();
        schedule.add_stage(ROLLBACK_DEFAULT, SystemStage::single_threaded());
        let ggrs_stage = GGRSStage::new(schedule);
        app.add_stage_before(CoreStage::Update, GGRS_UPDATE, ggrs_stage);
        // insert a rollback id provider
        app.insert_resource(RollbackIdProvider::default());
    }
}

/// Extension trait for the `App`.
pub trait GGRSApp {
    /// Adds the given `ggrs::SyncTestSession` to your app.
    fn with_synctest_session(&mut self, sess: SyncTestSession) -> &mut Self;

    /// Adds the given `ggrs::P2PSession` to your app.
    fn with_p2p_session(&mut self, sess: P2PSession) -> &mut Self;

    /// Adds the given `ggrs::P2PSpectatorSession` to your app.
    fn with_p2p_spectator_session(&mut self, sess: P2PSpectatorSession) -> &mut Self;

    /// Registers a given system as the input system. This system should provide encoded inputs for a given player.
    fn with_input_system<Params>(
        &mut self,
        input_system: impl IntoSystem<PlayerHandle, Vec<u8>, Params>,
    ) -> &mut Self;

    /// Sets the fixed update frequency
    fn with_fps(&mut self, fps: u32) -> &mut Self;

    /// Registers a type of component for saving and loading during rollbacks.
    fn register_rollback_type<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration + Reflect + Default;

    /// Adds a system that is executed as part of the ggrs update.
    fn add_rollback_system<Params>(
        &mut self,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self;

    /// Adds a system set that is executed as part of the ggrs update.
    fn add_rollback_system_set(&mut self, system: SystemSet) -> &mut Self;

    /// Adds a system set to a specific stage inside the GGRS schedule.
    fn add_rollback_system_set_to_stage(
        &mut self,
        stage_label: impl StageLabel,
        system_set: SystemSet,
    ) -> &mut Self;

    /// Adds a system to a specific stage inside the GGRS schedule.
    fn add_rollback_system_to_stage<Params>(
        &mut self,
        stage_label: impl StageLabel,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self;

    /// Adds a stage into the GGRS schedule.
    fn add_rollback_stage<S: Stage>(&mut self, label: impl StageLabel, stage: S) -> &mut Self;

    /// Adds a stage into the GGRS schedule after another stage inside the GGRS schedule.
    fn add_rollback_stage_after<S: Stage>(
        &mut self,
        target: impl StageLabel,
        label: impl StageLabel,
        stage: S,
    ) -> &mut Self;

    /// Adds a stage into the GGRS schedule before another stage inside the GGRS schedule.
    fn add_rollback_stage_before<S: Stage>(
        &mut self,
        target: impl StageLabel,
        label: impl StageLabel,
        stage: S,
    ) -> &mut Self;
}

impl GGRSApp for App {
    fn with_synctest_session(&mut self, session: SyncTestSession) -> &mut Self {
        self.insert_resource(SessionType::SyncTestSession);
        self.insert_resource(session);
        self
    }

    fn with_p2p_session(&mut self, session: P2PSession) -> &mut Self {
        self.insert_resource(SessionType::P2PSession);
        self.insert_resource(session);
        self
    }

    fn with_p2p_spectator_session(&mut self, session: P2PSpectatorSession) -> &mut Self {
        self.insert_resource(SessionType::P2PSpectatorSession);
        self.insert_resource(session);
        self
    }

    fn with_input_system<Params>(
        &mut self,
        input_system: impl IntoSystem<PlayerHandle, Vec<u8>, Params>,
    ) -> &mut Self {
        let mut input_system = input_system.system();
        input_system.initialize(&mut self.world);
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.input_system = Some(Box::new(input_system));
        self
    }

    fn with_fps(&mut self, fps: u32) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.fps = fps;
        self
    }

    fn register_rollback_type<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration + Reflect + Default,
    {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");

        let mut registry = ggrs_stage.type_registry.write();

        registry.register::<T>();

        let registration = registry.get_mut(std::any::TypeId::of::<T>()).unwrap();
        registration.insert(<ReflectComponent as FromType<T>>::from_type());
        registration.insert(<ReflectResource as FromType<T>>::from_type());
        drop(registry);

        self
    }

    fn add_rollback_system<Params>(
        &mut self,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage
            .schedule
            .add_system_to_stage(ROLLBACK_DEFAULT, system);
        self
    }

    fn add_rollback_system_set(&mut self, system_set: SystemSet) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage
            .schedule
            .add_system_set_to_stage(ROLLBACK_DEFAULT, system_set);
        self
    }

    fn add_rollback_system_to_stage<Params>(
        &mut self,
        stage_label: impl StageLabel,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.schedule.add_system_to_stage(stage_label, system);
        self
    }

    fn add_rollback_system_set_to_stage(
        &mut self,
        stage_label: impl StageLabel,
        system_set: SystemSet,
    ) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage
            .schedule
            .add_system_set_to_stage(stage_label, system_set);
        self
    }

    fn add_rollback_stage<S: Stage>(&mut self, label: impl StageLabel, stage: S) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.schedule.add_stage(label, stage);
        self
    }

    fn add_rollback_stage_after<S: Stage>(
        &mut self,
        target: impl StageLabel,
        label: impl StageLabel,
        stage: S,
    ) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.schedule.add_stage_after(target, label, stage);
        self
    }

    fn add_rollback_stage_before<S: Stage>(
        &mut self,
        target: impl StageLabel,
        label: impl StageLabel,
        stage: S,
    ) -> &mut Self {
        let ggrs_stage = self
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        ggrs_stage.schedule.add_stage_before(target, label, stage);
        self
    }
}

pub trait CommandsExt {
    fn start_p2p_session(&mut self, session: P2PSession);
    fn start_p2p_spectator_session(&mut self, session: P2PSpectatorSession);
    fn start_synctest_session(&mut self, session: SyncTestSession);
}

impl CommandsExt for Commands<'_, '_> {
    fn start_p2p_session(&mut self, session: P2PSession) {
        self.add(StartP2PSessionCommand(session));
    }

    fn start_p2p_spectator_session(&mut self, session: P2PSpectatorSession) {
        self.add(StartP2PSpectatorSessionCommand(session));
    }

    fn start_synctest_session(&mut self, session: SyncTestSession) {
        self.add(StartSyncTestSessionCommand(session));
    }
}

struct StartP2PSpectatorSessionCommand(P2PSpectatorSession);

impl Command for StartP2PSessionCommand {
    fn write(mut self, world: &mut World) {
        // caller is responsible that the session is either already running...
        if self.0.current_state() == SessionState::Initializing {
            // ...or ready to be started
            self.0.start_session().unwrap();
        }
        world.insert_resource(self.0);
        world.insert_resource(SessionType::P2PSession);
    }
}

struct StartP2PSessionCommand(P2PSession);

impl Command for StartP2PSpectatorSessionCommand {
    fn write(mut self, world: &mut World) {
        // caller is responsible that the session is either already running...
        if self.0.current_state() == SessionState::Initializing {
            // ...or ready to be started
            self.0.start_session().unwrap();
        }
        world.insert_resource(self.0);
        world.insert_resource(SessionType::P2PSpectatorSession);
    }
}

struct StartSyncTestSessionCommand(SyncTestSession);

impl Command for StartSyncTestSessionCommand {
    fn write(self, world: &mut World) {
        world.insert_resource(self.0);
        world.insert_resource(SessionType::SyncTestSession);
    }
}

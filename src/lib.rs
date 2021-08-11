use bevy::{
    ecs::schedule::{ShouldRun, SystemDescriptor},
    prelude::*,
    reflect::GetTypeRegistration,
};
use ggrs::PlayerHandle;
use stage::GGRSStage;

pub(crate) mod stage;
pub(crate) mod world_snapshot;

/// Stage label for the Custom GGRS Stage.
pub const GGRS_UPDATE: &str = "ggrs_update";
const GGRS_ADVANCE_FRAME: &str = "ggrs_advance_frame";

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

pub struct Rollback {
    id: u32,
}

impl Rollback {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub const fn id(&self) -> u32 {
        self.id
    }
}

#[derive(Default)]
pub struct RollbackIdProvider {
    next_id: u32,
}

impl RollbackIdProvider {
    pub fn next_id(&mut self) -> u32 {
        if self.next_id >= u32::MAX {
            panic!("RollbackIdProvider: u32::MAX has been reached.");
        }
        let ret = self.next_id;
        self.next_id += 1;
        ret
    }
}

pub struct GGRSPlugin;

impl Plugin for GGRSPlugin {
    fn build(&self, app: &mut AppBuilder) {
        // everything for the GGRS stage, where all rollback systems will be executed
        let mut schedule = Schedule::default();
        schedule.add_stage(GGRS_ADVANCE_FRAME, SystemStage::single_threaded());
        let mut ggrs_stage = GGRSStage::default();
        ggrs_stage.schedule = schedule;
        app.add_stage_before(CoreStage::Update, GGRS_UPDATE, ggrs_stage);
    }
}

pub trait GGRSAppBuilder {
    fn with_session_type(&mut self, session_type: SessionType) -> &mut Self;

    fn with_rollback_run_criteria(
        &mut self,
        run_criteria: impl System<In = (), Out = ShouldRun>,
    ) -> &mut Self;

    fn with_input_system(
        &mut self,
        input_fn: impl System<In = PlayerHandle, Out = Vec<u8>>,
    ) -> &mut Self;

    fn register_rollback_type<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration;

    fn add_rollback_system(&mut self, system: impl Into<SystemDescriptor>) -> &mut Self;

    fn add_rollback_system_set(&mut self, system: SystemSet) -> &mut Self;
}

impl GGRSAppBuilder for AppBuilder {
    fn with_session_type(&mut self, session_type: SessionType) -> &mut Self {
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage.session_type = session_type;
        self
    }

    fn with_rollback_run_criteria(
        &mut self,
        mut run_criteria: impl System<In = (), Out = ShouldRun>,
    ) -> &mut Self {
        run_criteria.initialize(self.world_mut());
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage.run_criteria = Some(Box::new(run_criteria));
        self
    }

    fn with_input_system(
        &mut self,
        mut input_system: impl System<In = PlayerHandle, Out = Vec<u8>>,
    ) -> &mut Self {
        input_system.initialize(self.world_mut());
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage.input_system = Some(Box::new(input_system));
        self
    }

    fn add_rollback_system(&mut self, system: impl Into<SystemDescriptor>) -> &mut Self {
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage
            .schedule
            .add_system_to_stage(GGRS_ADVANCE_FRAME, system);
        self
    }

    fn add_rollback_system_set(&mut self, system: SystemSet) -> &mut Self {
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage
            .schedule
            .add_system_set_to_stage(GGRS_ADVANCE_FRAME, system);
        self
    }

    fn register_rollback_type<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration,
    {
        let stage = self
            .app
            .schedule
            .get_stage_mut::<GGRSStage>(&GGRS_UPDATE)
            .expect("No GGRSStage found! Did you install the GGRSPlugin?");
        stage.type_registry.write().register::<T>();
        self
    }
}

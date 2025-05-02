//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
//!
//! See [`GgrsPlugin`] for getting started.
#![allow(clippy::type_complexity)] // Suppress warnings around Query

use bevy::{
    ecs::schedule::{ExecutorKind, LogLevel, ScheduleBuildSettings, ScheduleLabel},
    input::InputSystem,
    platform::collections::HashMap,
    prelude::*,
};
use core::time::Duration;
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, hash::Hash, marker::PhantomData, net::SocketAddr};

pub use ggrs;

pub use snapshot::*;
pub use time::*;

pub(crate) mod schedule_systems;
pub(crate) mod snapshot;
pub(crate) mod time;

pub mod prelude {
    pub use crate::{
        snapshot::prelude::*, AddRollbackCommandExtension, GgrsApp, GgrsConfig, GgrsPlugin,
        GgrsSchedule, GgrsTime, PlayerInputs, ReadInputs, Rollback, RollbackApp, Session,
    };
    pub use ggrs::{GgrsEvent, PlayerType, SessionBuilder};
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
    Input: Send + Sync + PartialEq + Serialize + for<'a> Deserialize<'a> + Default + Copy,
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
#[allow(clippy::large_enum_variant)]
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
    /// accumulated time. once enough time has been accumulated, an update is executed
    accumulator: Duration,
    /// boolean to see if we should run slow to let remote clients catch up
    run_slow: bool,
}

impl Default for FixedTimestepData {
    fn default() -> Self {
        Self {
            accumulator: Duration::ZERO,
            run_slow: false,
        }
    }
}

/// The maximum prediction window for this [`Session`], provided as a concrete [`Resource`].
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaxPredictionWindow(usize);

/// Inputs from local players. You have to fill this resource in the ReadInputs schedule.
#[derive(Resource)]
pub struct LocalInputs<C: Config>(pub HashMap<PlayerHandle, C::Input>);

/// Handles for the local players, you can use this when writing an input system.
#[derive(Resource, Default)]
pub struct LocalPlayers(pub Vec<PlayerHandle>);

/// Label for the schedule which reads the inputs for the current frame
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReadInputs;

/// GGRS plugin for bevy.
///
/// # Rollback
///
/// This will provide rollback management for the following items in the Bevy ECS:
/// - [Entities](`Entity`)
/// - [Parent] and [Children] components
/// - [Time]
///
/// To add more data to the rollback management, see the methods provided by [GgrsApp].
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::prelude::*;
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// // Add the GgrsPlugin with your input type
/// app.add_plugins(GgrsPlugin::<GgrsConfig<MyInputType>>::default());
///
/// // (optional) Override the default frequency of rollback game logic updates
/// app.set_rollback_schedule_fps(FPS);
///
/// // Provide a system to get player input
/// app.add_systems(ReadInputs, read_local_inputs);
///
/// // Add custom resources/components to be rolled back
/// app.rollback_component_with_clone::<Transform>();
///
/// // Once started, add your Session
/// app.insert_resource(session);
/// # }
/// ```
pub struct GgrsPlugin<C: Config> {
    /// phantom marker for ggrs config
    _marker: PhantomData<C>,
}

impl<C: Config> Default for GgrsPlugin<C> {
    fn default() -> Self {
        Self { _marker: default() }
    }
}

impl<C: Config> Plugin for GgrsPlugin<C> {
    fn build(&self, app: &mut App) {
        app.add_plugins(SnapshotPlugin)
            .init_resource::<MaxPredictionWindow>()
            .init_resource::<LocalPlayers>()
            .init_resource::<FixedTimestepData>()
            .init_schedule(ReadInputs)
            .edit_schedule(AdvanceWorld, |schedule| {
                // AdvanceWorld is mostly a facilitator for GgrsSchedule, so SingleThreaded avoids overhead
                // This can be overridden if desired.
                schedule.set_executor_kind(ExecutorKind::SingleThreaded);
            })
            .edit_schedule(GgrsSchedule, |schedule| {
                schedule.set_build_settings(ScheduleBuildSettings {
                    ambiguity_detection: LogLevel::Error,
                    ..default()
                });
            })
            .add_systems(
                AdvanceWorld,
                (|world: &mut World| world.run_schedule(GgrsSchedule))
                    .in_set(AdvanceWorldSet::Main),
            )
            .add_systems(
                PreUpdate,
                schedule_systems::run_ggrs_schedules::<C>.after(InputSystem),
            )
            .add_plugins((ChecksumPlugin, EntityChecksumPlugin, GgrsTimePlugin));
    }
}

/// exstension trait for [`App`] to add GGRS specific functionality
pub trait GgrsApp: RollbackApp {
    /// Set the frequency that game updates should be performed at.
    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self;
}

impl GgrsApp for App {
    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self {
        self.world_mut().insert_resource(RollbackFrameRate(fps));

        self
    }
}

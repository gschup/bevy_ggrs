//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
#![forbid(unsafe_code)] // let us try

use bevy::{
    ecs::{
        entity::MapEntities,
        schedule::{ExecutorKind, LogLevel, ScheduleBuildSettings, ScheduleLabel},
    },
    prelude::*,
    utils::{Duration, HashMap},
};
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use std::{fmt::Debug, hash::Hash, marker::PhantomData, net::SocketAddr};

pub use ggrs;

pub use rollback::*;
pub use snapshot::*;
pub use time::*;

pub(crate) mod rollback;
pub(crate) mod schedule_systems;
pub(crate) mod snapshot;
pub(crate) mod time;

pub mod prelude {
    pub use crate::{
        snapshot::prelude::*, AddRollbackCommandExtension, GgrsApp, GgrsConfig, GgrsPlugin,
        GgrsSchedule, PlayerInputs, ReadInputs, Rollback, Session,
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

/// Keeps track of the current frame the rollback simulation is in
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RollbackFrameCount(i32);

impl From<RollbackFrameCount> for i32 {
    fn from(value: RollbackFrameCount) -> i32 {
        value.0
    }
}

/// The most recently confirmed frame. Any information for frames stored before this point can be safely discarded.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfirmedFrameCount(i32);

impl From<ConfirmedFrameCount> for i32 {
    fn from(value: ConfirmedFrameCount) -> i32 {
        value.0
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

/// Label for the schedule which loads and overwrites a snapshot of the world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

/// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

/// Label for the schedule which advances the current world to the next frame.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AdvanceWorld;

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
        app.init_resource::<RollbackFrameCount>()
            .init_resource::<ConfirmedFrameCount>()
            .init_resource::<MaxPredictionWindow>()
            .init_resource::<RollbackOrdered>()
            .init_resource::<LocalPlayers>()
            .init_resource::<FixedTimestepData>()
            .init_schedule(ReadInputs)
            .init_schedule(LoadWorld)
            .init_schedule(SaveWorld)
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
            .add_systems(PreUpdate, schedule_systems::run_ggrs_schedules::<C>)
            .add_plugins((
                SnapshotSetPlugin,
                ChecksumPlugin,
                EntitySnapshotPlugin,
                EntityChecksumPlugin,
                GgrsTimePlugin,
                ResourceSnapshotPlugin::<CloneStrategy<RollbackOrdered>>::default(),
                ComponentSnapshotPlugin::<ReflectStrategy<Parent>>::default(),
                ComponentMapEntitiesPlugin::<Parent>::default(),
                ComponentSnapshotPlugin::<ReflectStrategy<Children>>::default(),
                ComponentMapEntitiesPlugin::<Children>::default(),
            ));
    }
}

/// Extension trait to add the GGRS plugin idiomatically to Bevy Apps
pub trait GgrsApp {
    /// Registers a component type for saving and loading from the world. This
    /// uses [`Copy`] based snapshots for rollback.
    fn rollback_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component + Copy;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`Copy`] based snapshots for rollback.
    fn rollback_resource_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Copy;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component + Clone;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Clone;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`reflection`](`Reflect`) based snapshots for rollback.
    ///
    /// NOTE: Unlike previous versions of `bevy_ggrs`, this will no longer automatically
    /// apply entity mapping through the [`MapEntities`](`bevy::ecs::entity::MapEntities`) trait.
    /// If you require this behavior, see [`ComponentMapEntitiesPlugin`].
    fn rollback_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component + Reflect + FromWorld;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`reflection`](`Reflect`) based snapshots for rollback.
    ///
    /// NOTE: Unlike previous versions of `bevy_ggrs`, this will no longer automatically
    /// apply entity mapping through the [`MapEntities`](`bevy::ecs::entity::MapEntities`) trait.
    /// If you require this behavior, see [`ComponentMapEntitiesPlugin`].
    fn rollback_resource_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Reflect + FromWorld;

    /// Set the frequency that game updates should be performed at.
    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self;

    /// Adds a component type to the checksum generation pipeline using [`Hash`].
    fn checksum_component_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Component + Hash;

    /// Updates a component after rollback using [`MapEntities`].
    fn update_component_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Component + MapEntities;

    /// Adds a resource type to the checksum generation pipeline using [`Hash`].
    fn checksum_resource_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Hash;

    /// Updates a resource after rollback using [`MapEntities`].
    fn update_resource_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Resource + MapEntities;
}

impl GgrsApp for App {
    fn set_rollback_schedule_fps(&mut self, fps: usize) -> &mut Self {
        self.world.insert_resource(RollbackFrameRate(fps));

        self
    }

    fn rollback_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component + Reflect + FromWorld,
    {
        self.add_plugins(ComponentSnapshotPlugin::<ReflectStrategy<Type>>::default())
    }

    fn rollback_resource_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Reflect + FromWorld,
    {
        self.add_plugins(ResourceSnapshotPlugin::<ReflectStrategy<Type>>::default())
    }

    fn rollback_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component + Copy,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CopyStrategy<Type>>::default())
    }

    fn rollback_resource_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Copy,
    {
        self.add_plugins(ResourceSnapshotPlugin::<CopyStrategy<Type>>::default())
    }

    fn rollback_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component + Clone,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CloneStrategy<Type>>::default())
    }

    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Clone,
    {
        self.add_plugins(ResourceSnapshotPlugin::<CloneStrategy<Type>>::default())
    }

    fn checksum_component_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Component + Hash,
    {
        self.add_plugins(ComponentChecksumHashPlugin::<Type>::default())
    }

    fn update_component_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Component + MapEntities,
    {
        self.add_plugins(ComponentMapEntitiesPlugin::<Type>::default())
    }

    fn checksum_resource_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Hash,
    {
        self.add_plugins(ResourceChecksumHashPlugin::<Type>::default())
    }

    fn update_resource_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Resource + MapEntities,
    {
        self.add_plugins(ResourceMapEntitiesPlugin::<Type>::default())
    }
}

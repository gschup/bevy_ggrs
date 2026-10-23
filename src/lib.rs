//! bevy_ggrs is a bevy plugin for the P2P rollback networking library GGRS.
//!
//! See [`GgrsPlugin`] for getting started.
//! For an overview of the internals, see the
//! [architecture doc](https://github.com/gschup/bevy_ggrs/blob/main/docs/architecture.md).
#![warn(missing_docs)]
#![allow(clippy::type_complexity)] // Suppress warnings around Query

use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::{
    ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    input::InputSystems,
    platform::collections::HashMap,
    prelude::*,
};
use core::time::Duration;
pub use ggrs;
use ggrs::{Config, InputStatus, P2PSession, PlayerHandle, SpectatorSession, SyncTestSession};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, hash::Hash, marker::PhantomData, net::SocketAddr};

pub use bevy_ggrs_derive::{Rollback, RollbackEq};
pub use snapshot::*;
pub use time::*;

pub(crate) mod schedule_systems;
pub(crate) mod snapshot;
pub(crate) mod time;

/// Convenient re-exports of the most commonly used types. Glob-import this to get started.
pub mod prelude {
    pub use crate::{
        GgrsConfig, GgrsPlugin, GgrsSchedule, GgrsTime, PlayerInputs, ReadInputs, ReflectRollback,
        Rollback, RollbackApp, RollbackFrameRate, RollbackId, RollbackRegistration, Session,
        SyncTestMismatch, snapshot::prelude::*,
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
    type InputPredictor = ggrs::PredictRepeatLast;
}

const DEFAULT_FPS: usize = 60;

/// The schedule that runs your rollback game logic each GGRS frame.
///
/// Systems added to this schedule will be saved and rolled back by bevy_ggrs.
/// It runs inside [`AdvanceWorld`] and inherits its ambiguity detection settings
/// (set to [`LogLevel::Error`](`bevy::ecs::schedule::LogLevel`) by default).
///
/// Add your gameplay systems here:
///
/// ```rust,ignore
/// app.add_systems(GgrsSchedule, (move_players, apply_inputs).chain());
/// ```
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct GgrsSchedule;

/// Defines the Session that the GGRS Plugin should expect as a resource.
#[allow(clippy::large_enum_variant)]
#[derive(Resource)]
pub enum Session<T: Config> {
    /// A local determinism-check session that resimulates every frame to verify rollback correctness.
    SyncTest(SyncTestSession<T>),
    /// A peer-to-peer session with rollback between connected players.
    P2P(P2PSession<T>),
    /// A spectator session that follows a P2P game without participating in input.
    Spectator(SpectatorSession<T>),
}

/// A resource holding the inputs for all players in the current GGRS frame.
///
/// Each entry is a `(Input, `[`InputStatus`]`)` pair. The [`InputStatus`] indicates
/// whether the input was received, predicted, or is from a disconnected player.
///
/// This resource is populated by bevy_ggrs before [`GgrsSchedule`] runs and should
/// be read by your input-handling systems.
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

/// Triggered when a [`SyncTestSession`] detects a checksum mismatch after
/// rollback resimulation. This means the resimulated state diverged from
/// the original — indicating a rollback correctness issue.
///
/// Observe this event to handle desyncs in tests:
///
/// ```rust,ignore
/// app.world_mut().add_observer(|trigger: On<SyncTestMismatch>| {
///     panic!("Desync at frame {}: mismatched frames {:?}",
///         trigger.event().current_frame, trigger.event().mismatched_frames);
/// });
/// ```
#[derive(Event, Debug, Clone)]
pub struct SyncTestMismatch {
    /// The frame at which the mismatch was detected.
    pub current_frame: ggrs::Frame,
    /// The frames whose checksums did not match.
    pub mismatched_frames: Vec<ggrs::Frame>,
}

/// Inputs from local players. You have to fill this resource in the ReadInputs schedule.
#[derive(Resource)]
pub struct LocalInputs<C: Config>(pub HashMap<PlayerHandle, C::Input>);

/// Handles for the local players, you can use this when writing an input system.
#[derive(Resource, Default)]
pub struct LocalPlayers(pub Vec<PlayerHandle>);

/// Label for the schedule which reads the inputs for the current frame
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReadInputs;

/// A [`SystemSet`] label for the system that drives all GGRS schedules each Bevy frame.
///
/// Use this to order your systems relative to the GGRS update loop.
/// By default this set runs in [`PreUpdate`], after [`InputSystems`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct RunGgrsSystems;

/// GGRS plugin for bevy.
///
/// # Rollback
///
/// This will provide rollback management for the following items in the Bevy ECS:
/// - [Entities](`Entity`)
/// - [`ChildOf`] and [`Children`] components
/// - [`Time`]
///
/// To add more data to the rollback management, see the methods provided by [`RollbackApp`].
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
/// // (optional) Override the default frequency (60) of rollback game logic updates
/// app.insert_resource(RollbackFrameRate(FPS));
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
    schedule: Interned<dyn ScheduleLabel>,
    /// phantom marker for ggrs config
    _marker: PhantomData<C>,
}

impl<C: Config> GgrsPlugin<C> {
    /// Creates a new [`GgrsPlugin`] that runs the GGRS update loop in the given `schedule`.
    ///
    /// Use this when you need GGRS to run in a schedule other than the default [`PreUpdate`].
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
            _marker: default(),
        }
    }
}

impl<C: Config> Default for GgrsPlugin<C> {
    /// Creates a [`GgrsPlugin`] that runs the GGRS update loop in [`PreUpdate`] (the recommended default).
    fn default() -> Self {
        Self {
            schedule: PreUpdate.intern(),
            _marker: default(),
        }
    }
}

impl<C: Config> Plugin for GgrsPlugin<C> {
    /// Registers all GGRS resources, schedules, and the session update system.
    fn build(&self, app: &mut App) {
        app.add_plugins(SnapshotPlugin)
            .init_resource::<MaxPredictionWindow>()
            .init_resource::<LocalPlayers>()
            .init_resource::<FixedTimestepData>()
            .init_schedule(ReadInputs)
            .edit_schedule(AdvanceWorld, |schedule| {
                // AdvanceWorld is mostly a facilitator for GgrsSchedule, so single threading avoids overhead
                // This can be overridden if desired.
                schedule.set_executor(SingleThreadedExecutor::new());
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
                    .in_set(AdvanceWorldSystems::Main),
            )
            .add_systems(
                self.schedule,
                schedule_systems::run_ggrs_schedules::<C>
                    .in_set(RunGgrsSystems)
                    .after(InputSystems), // If we are in PreUpdate, run after input is read
            )
            .add_plugins((ChecksumPlugin, EntityChecksumPlugin, GgrsTimePlugin));

        // Auto-reset rollback state when a new session is inserted.
        // This prevents stale snapshots/frame counts from a previous session
        // from causing rollback panics in the new session.
        //
        // We detect a genuinely new session via the absent -> present transition
        // of `Session<C>`, tracked by the private `SessionActive<C>` marker.
        // We deliberately do NOT use `resource_added::<Session<C>>`: on Bevy 0.19
        // `resource_scope` (used every frame by `run_ggrs_schedules`) re-inserts
        // `Session` with a fresh "added" tick, so `resource_added` would fire on
        // every frame and clear snapshots continuously.
        app.add_systems(
            self.schedule,
            reset_rollback_state::<C>.before(RunGgrsSystems),
        );
    }
}

/// Private marker tracking that the rollback state has been initialized for the
/// currently-present [`Session<C>`]. Present while a session is active; removed
/// once the session is gone, so the next session triggers a fresh reset.
#[derive(Resource)]
struct SessionActive<C: Config>(PhantomData<fn() -> C>);

/// Resets rollback state on the absent -> present transition of [`Session<C>`].
///
/// Clears all snapshot stores, resets the frame counter, and removes the
/// stale confirmed frame count so that the new session starts clean. Runs every
/// frame but only does work when a session has just started (or just ended).
fn reset_rollback_state<C: Config>(world: &mut World) {
    let has_session = world.contains_resource::<Session<C>>();
    let initialized = world.contains_resource::<SessionActive<C>>();

    match (has_session, initialized) {
        // Session ended: forget it so the next one resets.
        (false, true) => {
            world.remove_resource::<SessionActive<C>>();
        }
        // New session just inserted: reset rollback state.
        (true, false) => {
            world.insert_resource(SessionActive::<C>(PhantomData));
            let old_frame = world.resource::<RollbackFrameCount>().0;
            world.resource_mut::<RollbackFrameCount>().0 = 0;
            world.remove_resource::<ConfirmedFrameCount>();
            world.trigger(ClearSnapshots);
            world.insert_resource(Time::new_with(GgrsTime));
            info!("Reset rollback state for new session (was at frame {old_frame})");
        }
        // Steady state (running session, or no session): nothing to do.
        _ => {}
    }
}

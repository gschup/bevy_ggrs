use crate::DEFAULT_FPS;
use bevy::{ecs::schedule::ScheduleLabel, platform::collections::HashMap, prelude::*};
use seahash::SeaHasher;
use std::{collections::VecDeque, marker::PhantomData};

mod checksum;
mod childof_snapshot;
mod component_checksum;
mod component_map;
mod component_snapshot;
mod despawn;
mod entity;
mod entity_checksum;
mod resource_checksum;
mod resource_map;
mod resource_snapshot;
mod rollback;
mod rollback_app;
mod rollback_entity_map;
mod set;
mod strategy;

use crate::snapshot::despawn::RollbackDespawnPlugin;
pub use checksum::*;
pub use childof_snapshot::*;
pub use component_checksum::*;
pub use component_map::*;
pub use component_snapshot::*;
pub use entity::*;
pub use entity_checksum::*;
pub use resource_checksum::*;
pub use resource_map::*;
pub use resource_snapshot::*;
pub use rollback::*;
pub use rollback_app::*;
pub use rollback_entity_map::*;
pub use set::*;
pub use strategy::*;

pub mod prelude {
    pub use super::{Checksum, LoadWorldSystems, SaveWorldSystems};
}

/// Label for the schedule which loads and overwrites a snapshot of the world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LoadWorld;

/// Label for the schedule which saves a snapshot of the current world.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SaveWorld;

/// Label for the schedule which advances the current world to the next frame.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AdvanceWorld;

/// Keeps track of the current frame the rollback simulation is in
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RollbackFrameCount(pub i32);

impl From<RollbackFrameCount> for i32 {
    fn from(value: RollbackFrameCount) -> i32 {
        value.0
    }
}

/// The most recently confirmed frame. Any information for frames stored before this point can be safely discarded.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfirmedFrameCount(pub(crate) i32);

impl From<ConfirmedFrameCount> for i32 {
    fn from(value: ConfirmedFrameCount) -> i32 {
        value.0
    }
}

/// Typical [`Resource`] used to store snapshots for a [`Resource`] `R` as the type `As`.
/// For most types, the default `As = R` will suffice.
pub type GgrsResourceSnapshots<R, As = R> = GgrsSnapshots<R, Option<As>>;

/// Typical [`Resource`] used to store snapshots for a [`Component`] `C` as the type `As`.
/// For most types, the default `As = C` will suffice.
pub type GgrsComponentSnapshots<C, As = C> = GgrsSnapshots<C, GgrsComponentSnapshot<C, As>>;

/// Collection of snapshots for a type `For`, stored as `As`
#[derive(Resource)]
pub struct GgrsSnapshots<For, As = For> {
    /// Queue of snapshots, newest at the front, oldest at the back.
    /// Separate from `frames`` to avoid padding.
    snapshots: VecDeque<As>,
    /// Queue of frames, newest at the front, oldest at the back.
    /// Separate from `snapshots`` to avoid padding.
    frames: VecDeque<i32>,
    /// Maximum amount of snapshots to store at any one time
    depth: usize,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsSnapshots<For, As> {
    fn default() -> Self {
        Self {
            snapshots: VecDeque::with_capacity(DEFAULT_FPS),
            frames: VecDeque::with_capacity(DEFAULT_FPS),
            depth: DEFAULT_FPS, // TODO: Make sensible choice here
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsSnapshots<For, As> {
    /// Updates the capacity of this storage to the provided depth.
    pub fn set_depth(&mut self, depth: usize) -> &mut Self {
        self.depth = depth;

        // Greedy allocation to avoid allocating at a more sensitive time.
        if self.snapshots.capacity() < self.depth {
            let additional = self.depth - self.snapshots.capacity();
            self.snapshots.reserve(additional);
        }

        if self.frames.capacity() < self.depth {
            let additional = self.depth - self.frames.capacity();
            self.frames.reserve(additional);
        }

        self
    }

    /// Get the current capacity of this snapshot storage.
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Push a new snapshot for the provided frame. If the frame is earlier than any
    /// currently stored snapshots, those snapshots will be discarded.
    pub fn push(&mut self, frame: i32, snapshot: As) -> &mut Self {
        debug_assert_eq!(
            self.snapshots.len(),
            self.frames.len(),
            "Snapshot and Frame queues must always be in sync"
        );

        loop {
            let Some(&current) = self.frames.front() else {
                break;
            };

            // Handle the possibility of wrapping i32
            let wrapped = current.abs_diff(frame) > u32::MAX / 2;
            let current_after_frame = current >= frame && !wrapped;
            let current_after_frame_wrapped = frame >= current && wrapped;

            if current_after_frame || current_after_frame_wrapped {
                self.snapshots.pop_front().unwrap();
                self.frames.pop_front().unwrap();
            } else {
                break;
            }
        }

        self.snapshots.push_front(snapshot);
        self.frames.push_front(frame);

        while self.snapshots.len() > self.depth {
            self.snapshots.pop_back().unwrap();
            self.frames.pop_back().unwrap();
        }

        self
    }

    /// Confirms a snapshot as being stable across clients. Snapshots from before this
    /// point are discarded as no longer required.
    pub fn confirm(&mut self, confirmed_frame: i32) -> &mut Self {
        debug_assert_eq!(
            self.snapshots.len(),
            self.frames.len(),
            "Snapshot and Frame queues must always be in sync"
        );

        while let Some(&frame) = self.frames.back() {
            if frame < confirmed_frame {
                self.snapshots.pop_back().unwrap();
                self.frames.pop_back().unwrap();
            } else {
                break;
            }
        }

        self
    }

    /// Rolls back to the provided frame, discarding snapshots taken after the rollback point.
    pub fn rollback(&mut self, frame: i32) -> &mut Self {
        loop {
            let Some(&current) = self.frames.front() else {
                // TODO: A panic may not be appropriate here, but suitable for now.
                panic!("Could not rollback to {frame}: no snapshot at that moment could be found.");
            };

            if current != frame {
                self.snapshots.pop_front().unwrap();
                self.frames.pop_front().unwrap();
            } else {
                break;
            }
        }

        self
    }

    /// Get the current snapshot. Use `rollback(frame)` to first select a frame to rollback to.
    pub fn get(&self) -> &As {
        self.snapshots.front().unwrap()
    }

    /// Get a particular snapshot if it exists.
    pub fn peek(&self, frame: i32) -> Option<&As> {
        let (index, _) = self
            .frames
            .iter()
            .enumerate()
            .find(|&(_, &saved_frame)| saved_frame == frame)?;
        self.snapshots.get(index)
    }

    /// A system which automatically confirms the [`ConfirmedFrameCount`], discarding older snapshots.
    pub fn discard_old_snapshots(
        mut snapshots: ResMut<Self>,
        confirmed_frame: Option<Res<ConfirmedFrameCount>>,
    ) where
        For: Send + Sync + 'static,
        As: Send + Sync + 'static,
    {
        let Some(confirmed_frame) = confirmed_frame else {
            return;
        };

        snapshots.confirm(confirmed_frame.0);
    }
}

/// A storage type suitable for per-[`Entity`] snapshots, such as [`Component`] types.
pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: HashMap<Rollback, As>,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsComponentSnapshot<For, As> {
    fn default() -> Self {
        Self {
            snapshot: default(),
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsComponentSnapshot<For, As> {
    /// Create a new snapshot from a list of [`Rollback`] flags and stored [`Component`] types.
    pub fn new(components: impl IntoIterator<Item = (Rollback, As)>) -> Self {
        Self {
            snapshot: components.into_iter().collect(),
            ..default()
        }
    }

    /// Insert a single snapshot for the provided [`Rollback`].
    pub fn insert(&mut self, entity: Rollback, snapshot: As) -> &mut Self {
        self.snapshot.insert(entity, snapshot);
        self
    }

    /// Get a single snapshot for the provided [`Rollback`].
    pub fn get(&self, entity: &Rollback) -> Option<&As> {
        self.snapshot.get(entity)
    }

    /// Iterate over all stored snapshots.
    pub fn iter(&self) -> impl Iterator<Item = (&Rollback, &As)> + '_ {
        self.snapshot.iter()
    }
}

/// Returns a hasher built using the `seahash` library appropriate for creating portable checksums.
pub fn checksum_hasher() -> SeaHasher {
    SeaHasher::new()
}

/// This plugin sets up the [`LoadWorld`], [`SaveWorld`], and [`AdvanceWorld`]
/// schedules and adds the required systems and resources for basic rollback
/// functionality.
///
/// This is independent of the GGRS plugin and can be used with any Bevy app,
/// including tests and benchmarks.
pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SnapshotSetPlugin)
            .init_resource::<RollbackOrdered>()
            .init_resource::<RollbackFrameCount>()
            .init_resource::<ConfirmedFrameCount>()
            .init_schedule(LoadWorld)
            .init_schedule(SaveWorld)
            .init_schedule(AdvanceWorld)
            .add_plugins((
                EntitySnapshotPlugin,
                ResourceSnapshotPlugin::<CloneStrategy<RollbackOrdered>>::default(),
                ChildOfSnapshotPlugin,
                RollbackDespawnPlugin,
            ));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bevy::prelude::*;

    use super::{AdvanceWorld, LoadWorld, RollbackFrameCount, SaveWorld};

    /// Saves the world by running the [`SaveWorld`] schedule.
    pub(crate) fn save_world(world: &mut World) {
        world.run_schedule(SaveWorld);
    }

    /// Advances the world by one frame, running the [`AdvanceWorld`] schedule.
    ///
    /// assumes input has already been updated
    pub(crate) fn advance_frame(world: &mut World) -> i32 {
        let mut frame_count = world
            .get_resource_mut::<RollbackFrameCount>()
            .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?");
        frame_count.0 += 1;
        let frame = frame_count.0;
        world.run_schedule(AdvanceWorld);
        frame
    }

    /// Loads the world from the provided frame, by running the [`LoadWorld`] schedule.
    pub(crate) fn load_world(world: &mut World, frame: i32) {
        world
            .get_resource_mut::<RollbackFrameCount>()
            .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
            .0 = frame;
        world.run_schedule(LoadWorld);
    }
}

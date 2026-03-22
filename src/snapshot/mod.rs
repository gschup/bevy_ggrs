//! Core snapshot infrastructure for bevy_ggrs.
//!
//! This module exposes the three fundamental schedules that drive the rollback loop —
//! [`SaveWorld`], [`LoadWorld`], and [`AdvanceWorld`] — together with the snapshot storage
//! types ([`GgrsSnapshots`], [`GgrsComponentSnapshot`]) and the top-level
//! [`SnapshotPlugin`] that wires them all together.
//!
//! Most users interact with this module indirectly through [`RollbackApp`] and
//! [`GgrsPlugin`](`crate::GgrsPlugin`), but the types here are public so that
//! advanced users can build custom snapshot behaviour.

use crate::{DEFAULT_FPS, MaxPredictionWindow};
use bevy::{ecs::schedule::ScheduleLabel, platform::collections::HashMap, prelude::*};
use seahash::SeaHasher;
use std::{collections::VecDeque, marker::PhantomData};

mod checksum;
mod childof_snapshot;
mod component_checksum;
mod component_map;
mod component_snapshot;
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
pub struct ConfirmedFrameCount(pub i32);

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
            snapshots: VecDeque::new(),
            frames: VecDeque::new(),
            depth: DEFAULT_FPS, // Synced to MaxPredictionWindow before every save via sync_depth
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
    ///
    /// # Panics
    ///
    /// Panics if no snapshot exists for `frame`. Ensure snapshots are stored at least as far
    /// back as the maximum prediction window to avoid this.
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
        self.snapshots
            .front()
            .expect("no snapshot available — call rollback(frame) before get()")
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

    /// A system which syncs the snapshot depth to [`MaxPredictionWindow`].
    /// Runs before each save to ensure snapshots are never evicted prematurely
    /// when the prediction window exceeds the default depth.
    pub fn sync_depth(mut snapshots: ResMut<Self>, max_prediction: Option<Res<MaxPredictionWindow>>)
    where
        For: Send + Sync + 'static,
        As: Send + Sync + 'static,
    {
        let Some(max_prediction) = max_prediction else {
            return;
        };

        snapshots.set_depth(max_prediction.0);
    }
}

/// A storage type suitable for per-[`Entity`] snapshots, such as [`Component`] types.
pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: HashMap<RollbackId, As>,
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
    pub fn new(components: impl IntoIterator<Item = (RollbackId, As)>) -> Self {
        Self {
            snapshot: components.into_iter().collect(),
            ..default()
        }
    }

    /// Insert a single snapshot for the provided [`Rollback`].
    pub fn insert(&mut self, entity: RollbackId, snapshot: As) -> &mut Self {
        self.snapshot.insert(entity, snapshot);
        self
    }

    /// Get a single snapshot for the provided [`Rollback`].
    pub fn get(&self, entity: &RollbackId) -> Option<&As> {
        self.snapshot.get(entity)
    }

    /// Iterate over all stored snapshots.
    pub fn iter(&self) -> impl Iterator<Item = (&RollbackId, &As)> + '_ {
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
    /// Registers the rollback schedules, frame-count resources, and core snapshot plugins.
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
            ));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bevy::prelude::*;

    use super::{AdvanceWorld, GgrsSnapshots, LoadWorld, RollbackFrameCount, SaveWorld};

    // ---- GgrsSnapshots unit tests ----

    type Snap = GgrsSnapshots<u32, u32>;

    fn snap_with_depth(depth: usize) -> Snap {
        let mut s = Snap::default();
        s.set_depth(depth);
        s
    }

    // --- push ---

    /// Pushing a single frame stores it and makes it retrievable via peek.
    #[test]
    fn push_single_frame() {
        let mut s = snap_with_depth(8);
        s.push(0, 42);
        assert_eq!(s.peek(0), Some(&42));
    }

    /// Frames pushed in ascending order are all retained up to depth.
    #[test]
    fn push_ascending_frames_retained() {
        let mut s = snap_with_depth(8);
        for i in 0..5_i32 {
            s.push(i, i as u32 * 10);
        }
        for i in 0..5_i32 {
            assert_eq!(s.peek(i), Some(&(i as u32 * 10)));
        }
    }

    /// When depth is exceeded, the oldest frames are evicted.
    #[test]
    fn push_evicts_oldest_when_depth_exceeded() {
        let mut s = snap_with_depth(3);
        for i in 0..5_i32 {
            s.push(i, i as u32);
        }
        // Only frames 2, 3, 4 should survive
        assert!(s.peek(0).is_none());
        assert!(s.peek(1).is_none());
        assert_eq!(s.peek(2), Some(&2));
        assert_eq!(s.peek(3), Some(&3));
        assert_eq!(s.peek(4), Some(&4));
    }

    /// Pushing an older frame discards any snapshots newer than it.
    #[test]
    fn push_older_frame_discards_newer() {
        let mut s = snap_with_depth(8);
        s.push(5, 50);
        s.push(6, 60);
        s.push(7, 70);
        // Push frame 5 again (simulating rollback followed by re-save)
        s.push(5, 99);
        assert_eq!(s.peek(5), Some(&99));
        assert!(s.peek(6).is_none());
        assert!(s.peek(7).is_none());
    }

    /// Pushing the same frame twice replaces the old snapshot.
    #[test]
    fn push_same_frame_replaces() {
        let mut s = snap_with_depth(8);
        s.push(3, 10);
        s.push(3, 20);
        assert_eq!(s.peek(3), Some(&20));
    }

    // --- confirm ---

    /// Confirming a frame prunes all snapshots strictly before it.
    #[test]
    fn confirm_prunes_older_frames() {
        let mut s = snap_with_depth(8);
        for i in 0..6_i32 {
            s.push(i, i as u32);
        }
        s.confirm(3);
        assert!(s.peek(0).is_none());
        assert!(s.peek(1).is_none());
        assert!(s.peek(2).is_none());
        // Frame 3 itself is kept (confirm is exclusive lower bound)
        assert_eq!(s.peek(3), Some(&3));
        assert_eq!(s.peek(4), Some(&4));
        assert_eq!(s.peek(5), Some(&5));
    }

    /// Confirming beyond all stored frames leaves the storage empty.
    #[test]
    fn confirm_beyond_all_frames_empties_storage() {
        let mut s = snap_with_depth(8);
        for i in 0..4_i32 {
            s.push(i, i as u32);
        }
        s.confirm(100);
        for i in 0..4_i32 {
            assert!(s.peek(i).is_none());
        }
    }

    /// Confirming on an empty storage does not panic.
    #[test]
    fn confirm_on_empty_does_not_panic() {
        let mut s: Snap = snap_with_depth(8);
        s.confirm(5); // should not panic
    }

    // --- rollback ---

    /// Rollback to an existing frame succeeds and positions the cursor there.
    #[test]
    fn rollback_to_existing_frame() {
        let mut s = snap_with_depth(8);
        for i in 0..5_i32 {
            s.push(i, i as u32 * 10);
        }
        s.rollback(2);
        assert_eq!(s.get(), &20);
    }

    /// Rollback discards snapshots newer than the target frame.
    #[test]
    fn rollback_discards_newer_frames() {
        let mut s = snap_with_depth(8);
        for i in 0..5_i32 {
            s.push(i, i as u32);
        }
        s.rollback(2);
        assert!(s.peek(3).is_none());
        assert!(s.peek(4).is_none());
        assert_eq!(s.peek(2), Some(&2));
    }

    /// Rollback to a missing frame panics.
    #[test]
    #[should_panic(expected = "Could not rollback to 99")]
    fn rollback_missing_frame_panics() {
        let mut s = snap_with_depth(8);
        s.push(0, 0);
        s.rollback(99);
    }

    // --- peek ---

    /// Peek returns None for a frame that was never stored.
    #[test]
    fn peek_missing_frame_returns_none() {
        let mut s = snap_with_depth(8);
        s.push(1, 10);
        assert!(s.peek(0).is_none());
        assert!(s.peek(2).is_none());
    }

    // --- i32 wraparound ---

    /// Pushing i32::MIN after i32::MAX is a forward step across the wrap boundary.
    /// History frames near i32::MAX are retained (they're older context), and i32::MIN
    /// is prepended as the newest snapshot.
    #[test]
    fn push_wraps_i32_max_to_min_retains_history() {
        let mut s = snap_with_depth(8);
        s.push(i32::MAX - 2, 1);
        s.push(i32::MAX - 1, 2);
        s.push(i32::MAX, 3);
        // i32::MIN is "after" i32::MAX in GGRS frame counting (forward wrap).
        // The old frames are history and must be retained.
        s.push(i32::MIN, 4);
        assert_eq!(s.peek(i32::MAX - 2), Some(&1));
        assert_eq!(s.peek(i32::MAX - 1), Some(&2));
        assert_eq!(s.peek(i32::MAX), Some(&3));
        assert_eq!(s.peek(i32::MIN), Some(&4));
    }

    /// Pushing i32::MAX after i32::MIN is a rollback across the wrap boundary.
    /// i32::MIN is "after" i32::MAX in frame time, so it must be evicted as a future snapshot.
    #[test]
    fn push_max_after_min_evicts_min_as_future() {
        let mut s = snap_with_depth(8);
        s.push(i32::MIN, 1);
        // Pushing MAX means rolling back to before the wrap — i32::MIN is a future frame.
        s.push(i32::MAX, 2);
        assert!(
            s.peek(i32::MIN).is_none(),
            "i32::MIN should be evicted as a future frame"
        );
        assert_eq!(s.peek(i32::MAX), Some(&2));
    }

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

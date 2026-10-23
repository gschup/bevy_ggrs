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

use crate::DEFAULT_FPS;
use bevy::{
    ecs::schedule::ScheduleLabel,
    ecs::{change_detection::Tick, reflect::AppTypeRegistry},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use seahash::SeaHasher;

use std::{collections::BTreeSet, marker::PhantomData, sync::Arc};

mod checksum;
mod checksum_diagnostics;
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
mod rollback_entity;
mod rollback_entity_map;
mod set;
mod staged_insert;
mod strategy;

pub use checksum::*;
pub use checksum_diagnostics::*;
pub use childof_snapshot::*;
pub use component_checksum::*;
pub use component_map::*;
pub use component_snapshot::*;
pub use despawn::*;
pub use entity::*;
pub use entity_checksum::*;
pub use resource_checksum::*;
pub use resource_map::*;
pub use resource_snapshot::*;
pub use rollback::*;
pub use rollback_app::*;
pub use rollback_entity::*;
pub use rollback_entity_map::*;
pub use set::*;
pub use staged_insert::*;
pub use strategy::*;

pub mod prelude {
    pub use super::despawn::{RollbackDespawnCommandExtension, RollbackDespawned};
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

/// Collection of snapshots for a type `For`, stored as `As`.
///
/// Snapshot depth (how many frames to retain) is controlled globally via
/// [`SnapshotDepth`], not per-storage. Eviction happens in
/// [`discard_old_snapshots`](Self::discard_old_snapshots).
#[derive(Resource)]
pub struct GgrsSnapshots<For, As = For> {
    snapshots: HashMap<i32, As>,
    /// The frame selected by `rollback()`, used by `get()`.
    current_frame: Option<i32>,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsSnapshots<For, As> {
    fn default() -> Self {
        Self {
            snapshots: HashMap::default(),
            current_frame: None,
            _phantom: PhantomData,
        }
    }
}

impl<For, As> GgrsSnapshots<For, As> {
    /// Remove all stored snapshots.
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.current_frame = None;
    }

    /// Store a snapshot for the provided frame, replacing any existing snapshot.
    pub fn push(&mut self, frame: i32, snapshot: As) -> &mut Self {
        self.snapshots.insert(frame, snapshot);
        self
    }

    /// Evict snapshots beyond the depth limit, keeping only the most recent `depth` unpinned frames.
    ///
    /// Pinned frames are never evicted regardless of the depth limit.
    pub fn evict(&mut self, depth: usize, pinned: &HashSet<i32>) {
        self.discard(Some(depth), pinned, None);
    }

    /// Discards unpinned snapshots from before `confirmed_frame` as no longer required.
    pub fn confirm(&mut self, confirmed_frame: i32, pinned: &HashSet<i32>) -> &mut Self {
        self.snapshots
            .retain(|&frame, _| frame >= confirmed_frame || pinned.contains(&frame));
        self
    }

    /// [`evict`](Self::evict) and [`confirm`](Self::confirm) in one pass over the stored frames.
    fn discard(&mut self, depth: Option<usize>, pinned: &HashSet<i32>, confirmed: Option<i32>) {
        let mut frames: Vec<i32> = self.snapshots.keys().copied().collect();
        frames.sort_unstable();
        for frame in frames_to_discard(&mut frames.into_iter().rev(), depth, pinned, confirmed) {
            self.snapshots.remove(&frame);
        }
    }

    /// Selects a frame to rollback to. Use `get()` to retrieve the snapshot.
    pub fn rollback(&mut self, frame: i32) -> &mut Self {
        assert!(
            self.snapshots.contains_key(&frame),
            "Could not rollback to {frame}: no snapshot at that moment could be found."
        );
        self.current_frame = Some(frame);
        self
    }

    /// Get the snapshot for the frame selected by `rollback()`.
    pub fn get(&self) -> &As {
        let frame = self
            .current_frame
            .expect("No frame selected. Call rollback() before get().");
        self.snapshots
            .get(&frame)
            .expect("Snapshot missing for selected frame")
    }

    /// Get a snapshot for a specific frame, if it exists.
    pub fn peek(&self, frame: i32) -> Option<&As> {
        self.snapshots.get(&frame)
    }

    /// A system which evicts old snapshots based on [`SnapshotDepth`] and
    /// confirms the [`ConfirmedFrameCount`]. Pinned frames (via [`PinnedFrames`])
    /// are never evicted or confirmed away.
    ///
    /// Which frames go is decided once per [`SaveWorld`] for every store, by
    /// [`plan_snapshot_discards`]; this only removes the frames it names.
    pub fn discard_old_snapshots(
        mut snapshots: ResMut<Self>,
        frames: Res<SnapshotFrames>,
        depth: Res<SnapshotDepth>,
        pinned: Res<PinnedFrames>,
        confirmed_frame: Option<Res<ConfirmedFrameCount>>,
    ) where
        For: Send + Sync + 'static,
        As: Send + Sync + 'static,
    {
        for frame in &frames.discard {
            snapshots.snapshots.remove(frame);
        }

        // Every frame a store holds was saved in `SaveWorld`, so the plan has
        // seen it. A store holding more frames than the plan was filled some
        // other way, and decides for itself rather than grow without bound.
        if snapshots.snapshots.len() > frames.saved.len() {
            snapshots.discard(depth.0, &pinned, confirmed_frame.map(|c| c.0));
        }
    }
}

/// The frames to discard, given every stored frame newest first: all but the
/// newest `depth` unpinned frames (none when `depth` is `None`), and every
/// unpinned frame before `confirmed`.
///
/// Not generic, so it is compiled once, in this crate, however many snapshot
/// types there are.
fn frames_to_discard(
    newest_first: &mut dyn Iterator<Item = i32>,
    depth: Option<usize>,
    pinned: &HashSet<i32>,
    confirmed: Option<i32>,
) -> Vec<i32> {
    let mut unpinned = 0;
    let mut discard = Vec::new();
    for frame in newest_first {
        if pinned.contains(&frame) {
            continue;
        }
        unpinned += 1;
        let beyond_depth = depth.is_some_and(|depth| unpinned > depth);
        let confirmed_away = confirmed.is_some_and(|confirmed| frame < confirmed);
        if beyond_depth || confirmed_away {
            discard.push(frame);
        }
    }
    discard
}

/// The frames the snapshot stores hold, and the ones this [`SaveWorld`] discards.
///
/// Every store saves the same frames and discards by the same rule
/// ([`SnapshotDepth`], [`PinnedFrames`], [`ConfirmedFrameCount`]), so the
/// rule runs once per save here, in [`plan_snapshot_discards`], and each
/// store's [`discard_old_snapshots`](GgrsSnapshots::discard_old_snapshots)
/// removes what it names. Run per store, the rule was a scan of every stored
/// frame, with a [`PinnedFrames`] lookup each, per snapshot type per frame.
#[derive(Resource, Default, Debug)]
pub struct SnapshotFrames {
    saved: BTreeSet<i32>,
    discard: Vec<i32>,
}

impl SnapshotFrames {
    /// The frames the stores hold, oldest first, before this save's discard.
    pub fn saved(&self) -> impl Iterator<Item = i32> + '_ {
        self.saved.iter().copied()
    }

    /// The frames this save discards from every store.
    pub fn discarded(&self) -> &[i32] {
        &self.discard
    }

    /// The newest held frame before `frame`, if any.
    pub fn newest_before(&self, frame: i32) -> Option<i32> {
        self.saved.range(..frame).next_back().copied()
    }
}

/// Decides which frames every snapshot store discards this [`SaveWorld`], and
/// records the frame about to be saved. Runs before
/// [`SaveWorldSystems::Snapshot`].
pub fn plan_snapshot_discards(
    mut frames: ResMut<SnapshotFrames>,
    frame: Res<RollbackFrameCount>,
    depth: Res<SnapshotDepth>,
    pinned: Res<PinnedFrames>,
    confirmed_frame: Option<Res<ConfirmedFrameCount>>,
) {
    let frames = &mut *frames;
    frames.discard = frames_to_discard(
        &mut frames.saved.iter().rev().copied(),
        depth.0,
        &pinned,
        confirmed_frame.map(|c| c.0),
    );
    for discarded in &frames.discard {
        frames.saved.remove(discarded);
    }
    frames.saved.insert(frame.0);
}

/// A storage type suitable for per-[`Entity`] snapshots, such as [`Component`] types.
///
/// The data is shared: cloning a snapshot is an `Arc` clone. A save that finds
/// nothing of its type changed since the snapshot the world holds pushes that
/// snapshot again rather than copying every component
/// ([`HeldComponentSnapshot`]).
pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: Arc<HashMap<RollbackId, As>>,
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

impl<For, As> Clone for GgrsComponentSnapshot<For, As> {
    fn clone(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsComponentSnapshot<For, As> {
    /// Create a new snapshot from a list of [`Rollback`] flags and stored [`Component`] types.
    pub fn new(components: impl IntoIterator<Item = (RollbackId, As)>) -> Self {
        Self {
            snapshot: Arc::new(components.into_iter().collect()),
            ..default()
        }
    }

    /// Get a single snapshot for the provided [`Rollback`].
    pub fn get(&self, entity: &RollbackId) -> Option<&As> {
        self.snapshot.get(entity)
    }

    /// Iterate over all stored snapshots.
    pub fn iter(&self) -> impl Iterator<Item = (&RollbackId, &As)> + '_ {
        self.snapshot.iter()
    }

    /// How many entities the snapshot holds.
    pub fn len(&self) -> usize {
        self.snapshot.len()
    }

    /// Whether the snapshot holds no entity.
    pub fn is_empty(&self) -> bool {
        self.snapshot.is_empty()
    }

    /// Whether `self` and `other` are the same snapshot, shared, not two
    /// that happen to hold equal data.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.snapshot, &other.snapshot)
    }
}

impl<For, As: Clone> GgrsComponentSnapshot<For, As> {
    /// Insert a single snapshot for the provided [`Rollback`]. Copies the
    /// data first if another snapshot shares it.
    pub fn insert(&mut self, entity: RollbackId, snapshot: As) -> &mut Self {
        Arc::make_mut(&mut self.snapshot).insert(entity, snapshot);
        self
    }
}

/// The snapshot of `For` the world's components equal, and the tick from
/// which that holds: the last save, or the last load that wrote them.
///
/// A save or load checks it first. With every component of the type no newer
/// than `tick`, and as many of them as the snapshot holds (so none was
/// removed, none added), the world still equals it: the save pushes it again
/// instead of copying, and a load of a frame that shares it writes nothing.
/// A load that wrote nothing leaves no change behind, so the ticks it
/// resimulates share too.
#[derive(Resource)]
pub struct HeldComponentSnapshot<For, As = For> {
    held: Option<(GgrsComponentSnapshot<For, As>, Tick)>,
}

impl<For, As> Default for HeldComponentSnapshot<For, As> {
    fn default() -> Self {
        Self { held: None }
    }
}

impl<For, As> HeldComponentSnapshot<For, As> {
    /// The held snapshot, if the world still equals it: `last_changed` is the
    /// change tick of every component of the type the world holds.
    pub fn unchanged(
        &self,
        last_changed: impl Iterator<Item = Tick>,
        this_run: Tick,
    ) -> Option<&GgrsComponentSnapshot<For, As>> {
        let (snapshot, tick) = self.held.as_ref()?;
        let mut count = 0;
        for changed in last_changed {
            if changed.is_newer_than(*tick, this_run) {
                return None;
            }
            count += 1;
        }
        (count == snapshot.len()).then_some(snapshot)
    }

    /// The world equals `snapshot` from `tick` on.
    pub fn set(&mut self, snapshot: GgrsComponentSnapshot<For, As>, tick: Tick) {
        self.held = Some((snapshot, tick));
    }

    /// Forget the held snapshot: the next save copies.
    pub fn clear(&mut self) {
        self.held = None;
    }
}

/// Returns a hasher built using the `seahash` library appropriate for creating portable checksums.
pub fn checksum_hasher() -> SeaHasher {
    SeaHasher::new()
}

/// Trigger this event to clear all snapshot stores.
///
/// Each snapshot plugin registers an observer that clears its own store
/// when this event fires. Use this when starting a new session to flush
/// stale snapshots from a previous session that used different frame
/// numbering.
///
/// # Example
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// fn reset_session(world: &mut World) {
///     world.trigger(bevy_ggrs::ClearSnapshots);
/// }
/// ```
#[derive(Event)]
pub struct ClearSnapshots;

/// Global snapshot depth limit. `None` means unbounded.
///
/// Defaults to `Some(60)` (~1 second at 60 fps). Insert this resource
/// before adding [`SnapshotPlugin`] to override the default, or mutate it
/// at runtime.
///
/// Read by [`discard_old_snapshots`](GgrsSnapshots::discard_old_snapshots).
#[derive(Resource)]
pub struct SnapshotDepth(pub Option<usize>);

/// Set of frame numbers that should never be evicted by
/// [`discard_old_snapshots`](GgrsSnapshots::discard_old_snapshots).
///
/// Use this to implement sparse checkpoint systems: pin a frame every N ticks
/// so that seeking backward can load a nearby checkpoint and resimulate forward.
///
/// The dense rolling window ([`SnapshotDepth`]) and pinned frames are independent:
/// `evict()` only counts unpinned frames against the depth limit.
#[derive(Resource, Default, Debug, Clone, Deref, DerefMut, Reflect)]
#[reflect(Resource)]
pub struct PinnedFrames(pub HashSet<i32>);

impl Default for SnapshotDepth {
    fn default() -> Self {
        Self(Some(DEFAULT_FPS))
    }
}

/// This plugin sets up the [`LoadWorld`], [`SaveWorld`], and [`AdvanceWorld`]
/// schedules and adds the required systems and resources for basic rollback
/// functionality.
///
/// Snapshot depth is controlled via the [`SnapshotDepth`] resource.
/// Insert it before adding this plugin to override the default (60 frames).
///
/// This is independent of the GGRS plugin and can be used with any Bevy app,
/// including tests and benchmarks.
pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    /// Registers the rollback schedules, frame-count resources, and core snapshot plugins.
    fn build(&self, app: &mut App) {
        app.init_resource::<SnapshotDepth>();
        app.init_resource::<PinnedFrames>();
        app.register_type::<PinnedFrames>();
        app.init_resource::<SnapshotFrames>()
            .add_systems(
                SaveWorld,
                plan_snapshot_discards
                    .after(SaveWorldSystems::Checksum)
                    .before(SaveWorldSystems::Snapshot),
            )
            .add_observer(
                |_: On<ClearSnapshots>, mut frames: ResMut<SnapshotFrames>| {
                    *frames = SnapshotFrames::default();
                },
            );
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

    fn finish(&self, app: &mut App) {
        if app.world().contains_resource::<SkipAutoRegistration>() {
            return;
        }

        let Some(registry) = app.world().get_resource::<AppTypeRegistry>() else {
            return;
        };

        let registry = registry.read();
        let registrations: Vec<fn(&mut App)> = registry
            .iter_with_data::<ReflectRollback>()
            .map(|(_, data)| data.register_fn)
            .collect();
        drop(registry);

        for register_fn in registrations {
            (register_fn)(app);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bevy::prelude::*;

    use super::{AdvanceWorld, GgrsSnapshots, LoadWorld, RollbackFrameCount, SaveWorld};

    // ---- GgrsSnapshots unit tests ----

    type Snap = GgrsSnapshots<u32, u32>;

    fn snap() -> Snap {
        Snap::default()
    }

    /// Empty pinned-frame set for `evict`/`confirm` in tests.
    fn no_pins() -> bevy::platform::collections::HashSet<i32> {
        bevy::platform::collections::HashSet::default()
    }

    // --- push ---

    /// When depth is exceeded, `evict` removes the oldest unpinned frames.
    #[test]
    fn evict_removes_oldest_when_depth_exceeded() {
        let mut s = snap();
        for i in 0..5_i32 {
            s.push(i, i as u32);
        }
        s.evict(3, &no_pins());
        // Only frames 2, 3, 4 should survive
        assert!(s.peek(0).is_none());
        assert!(s.peek(1).is_none());
        assert_eq!(s.peek(2), Some(&2));
        assert_eq!(s.peek(3), Some(&3));
        assert_eq!(s.peek(4), Some(&4));
    }

    /// Pushing the same frame twice replaces the old snapshot.
    #[test]
    fn push_same_frame_replaces() {
        let mut s = snap();
        s.push(3, 10);
        s.push(3, 20);
        assert_eq!(s.peek(3), Some(&20));
    }

    // --- confirm ---

    /// Confirming a frame prunes all snapshots strictly before it.
    #[test]
    fn confirm_prunes_older_frames() {
        let mut s = snap();
        for i in 0..6_i32 {
            s.push(i, i as u32);
        }
        s.confirm(3, &no_pins());
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
        let mut s = snap();
        for i in 0..4_i32 {
            s.push(i, i as u32);
        }
        s.confirm(100, &no_pins());
        for i in 0..4_i32 {
            assert!(s.peek(i).is_none());
        }
    }

    /// Confirming on an empty storage does not panic.
    #[test]
    fn confirm_on_empty_does_not_panic() {
        let mut s: Snap = snap();
        s.confirm(5, &no_pins()); // should not panic
    }

    // --- rollback ---

    /// Rollback to an existing frame succeeds and positions the cursor there.
    #[test]
    fn rollback_to_existing_frame() {
        let mut s = snap();
        for i in 0..5_i32 {
            s.push(i, i as u32 * 10);
        }
        s.rollback(2);
        assert_eq!(s.get(), &20);
    }

    /// Rollback to a missing frame panics.
    #[test]
    #[should_panic(expected = "Could not rollback to 99")]
    fn rollback_missing_frame_panics() {
        let mut s = snap();
        s.push(0, 0);
        s.rollback(99);
    }

    // --- i32 wraparound ---

    /// Pushing i32::MIN after i32::MAX is a forward step across the wrap boundary.
    /// History frames near i32::MAX are retained (they're older context), and i32::MIN
    /// is prepended as the newest snapshot.
    #[test]
    fn push_wraps_i32_max_to_min_retains_history() {
        let mut s = snap();
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

    // --- the per-save discard plan ---

    /// The frames a store holds, oldest first.
    fn stored<For, As>(s: &GgrsSnapshots<For, As>) -> Vec<i32> {
        let mut frames: Vec<i32> = s.snapshots.keys().copied().collect();
        frames.sort_unstable();
        frames
    }

    /// Every store keeps what it kept when each store ran the rule on its own:
    /// the newest `depth` unpinned frames before the one it saves, every
    /// pinned frame, and nothing unpinned before the confirmed frame.
    #[test]
    fn every_store_discards_by_the_plan() {
        use super::{
            ConfirmedFrameCount, GgrsComponentSnapshots, GgrsResourceSnapshots, PinnedFrames,
            RollbackOrdered, SnapshotDepth, SnapshotPlugin,
        };

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SnapshotDepth(Some(3)));
        app.add_plugins(SnapshotPlugin);
        app.update();
        let pinned = [1, 5];
        app.world_mut()
            .resource_mut::<PinnedFrames>()
            .extend(pinned);

        // The rule as each store ran it before the plan, step by step.
        let mut expected: Vec<i32> = Vec::new();
        for frame in 0..20 {
            let confirmed = (frame >= 12).then_some(10);
            if let Some(confirmed) = confirmed {
                app.world_mut()
                    .insert_resource(ConfirmedFrameCount(confirmed));
            }
            save_world(app.world_mut());

            let unpinned = |e: &Vec<i32>| e.iter().filter(|f| !pinned.contains(f)).count();
            while unpinned(&expected) > 3 {
                let oldest = expected.iter().position(|f| !pinned.contains(f)).unwrap();
                expected.remove(oldest);
            }
            if let Some(confirmed) = confirmed {
                expected.retain(|f| *f >= confirmed || pinned.contains(f));
            }
            expected.push(frame);

            let world = app.world();
            assert_eq!(
                stored(world.resource::<GgrsComponentSnapshots<Entity>>()),
                expected,
                "entity store after saving frame {frame}"
            );
            assert_eq!(
                stored(world.resource::<GgrsResourceSnapshots<RollbackOrdered>>()),
                expected,
                "resource store after saving frame {frame}"
            );
            advance_frame(app.world_mut());
        }
    }

    /// A store filled outside `SaveWorld` holds frames the plan never saw,
    /// and discards them by the same rule rather than keep them forever.
    #[test]
    fn a_store_the_plan_has_not_seen_discards_for_itself() {
        use super::{GgrsComponentSnapshots, SnapshotDepth, SnapshotPlugin};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SnapshotDepth(Some(3)));
        app.add_plugins(SnapshotPlugin);
        app.update();
        {
            let mut store = app
                .world_mut()
                .resource_mut::<GgrsComponentSnapshots<Entity>>();
            for frame in 0..10 {
                store.push(frame, default());
            }
        }
        app.world_mut().resource_mut::<RollbackFrameCount>().0 = 10;
        save_world(app.world_mut());
        assert_eq!(
            stored(app.world().resource::<GgrsComponentSnapshots<Entity>>()),
            vec![7, 8, 9, 10]
        );
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

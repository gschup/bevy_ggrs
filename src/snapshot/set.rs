//! System set definitions for the [`LoadWorld`], [`SaveWorld`], and [`AdvanceWorld`] schedules.
//!
//! These sets provide explicit ordering hooks so that plugins can interleave their systems
//! cleanly within the rollback loop. See [`LoadWorldSystems`], [`SaveWorldSystems`], and
//! [`AdvanceWorldSystems`] for the available sets and their documented order guarantees.

use bevy::prelude::*;

use crate::snapshot::{
    AdvanceWorld, LoadWorld, SaveWorld, StagedInserts, insert_staged_components,
};

/// Set for ordering systems during the [`LoadWorld`] schedule.
/// The most common option is [`LoadWorldSystems::ComponentData`], which is where [`Component`]
/// snapshots are loaded and applied to the [`World`]. [`Resource`] snapshots follow in
/// [`LoadWorldSystems::ResourceData`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum LoadWorldSystems {
    /// Removes any despawn markers if the loaded frame is before they were marked.
    /// See [`despawn module docs`](`crate::snapshot::despawn`).
    EntityResurrect,
    /// Recreate the [`Entity`] graph as it was during the frame to be rolled back to.
    /// When this set is complete, all entities that were alive during the snapshot
    /// frame have been recreated, and any that were not have been removed. If the
    /// [`Entity`] has changed ID, the new ID will be recorded. All this information will be
    /// made available in [`RollbackEntityMap`](`crate::RollbackEntityMap`).
    Entity,
    /// Flush any deferred operations
    EntityFlush,
    /// Recreate the stored [`Components`](`Component`) as they were during the frame to be
    /// rolled back to. Once [`LoadWorldSystems::ComponentDataFlush`] is complete, they are in their
    /// exact state during the snapshot.
    ///
    /// NOTE: At this point, [`Entity`] relationships may be broken, see [`LoadWorldSystems::Mapping`]
    /// for when those relationships are fixed.
    ComponentData,
    /// Flush any deferred operations, then insert the components the load systems staged,
    /// one insert per entity ([`insert_staged_components`]). A respawned entity's components
    /// land here, all at once, and their hooks and observers run.
    ComponentDataFlush,
    /// Recreate the stored [`Resources`](`Resource`) as they were during the frame to be
    /// rolled back to. When this set is complete, all [`Resources`](`Resource`) are in their
    /// exact state during the snapshot.
    ///
    /// Runs after [`LoadWorldSystems::ComponentDataFlush`] so that anything the component hooks
    /// wrote to a rolled-back resource there is overwritten: a `#[require]`d default
    /// inserted ahead of its snapshot value can take an id from a counter, for example.
    ResourceData,
    /// Flush any deferred operations
    ResourceDataFlush,
    /// Update all [`Components`](`Component`) and [`Resources`](`Resource`) to reflect the modified
    /// state of the rollback when compared to the original snapshot. For example, [`Entities`](`Entity`)
    /// which had to be recreated could not use the same ID, so any data referring to that ID is now invalid.
    /// Once this set completes, all data should now be coherent with the [`World`].
    Mapping,
}

/// Set for ordering systems during the [`SaveWorld`] schedule.
///
/// Systems run in the order `Checksum` → `Snapshot`. The total [`Checksum`](`crate::Checksum`)
/// for the frame is computed between the two sets.
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum SaveWorldSystems {
    /// Generate checksums for any tracked data.
    ///
    /// Within this set, it is expected that all data which will participate in the
    /// total checksum recorded for this frame will have updated/created a single [`Entity`]
    /// with a [`ChecksumPart`](`crate::ChecksumPart`) component, containing its contribution.
    ///
    /// The final [`Checksum`](`crate::Checksum`) for the frame will be produced after this set, but before
    /// the [`Snapshot`](`SaveWorldSystems::Snapshot`) set.
    Checksum,
    /// Saves a snapshot of the [`World`] in this state for future possible rollback.
    Snapshot,
}

/// Set for ordering systems during the [`AdvanceWorld`] schedule.
///
/// [`AdvanceWorld`] runs once per GGRS frame (including re-simulated frames during rollback).
/// Systems run in the order `First` → `Main` → `Last`, with [`ApplyDeferred`] inserted
/// between each pair.
///
/// [`GgrsSchedule`](`crate::GgrsSchedule`) is run inside [`AdvanceWorldSystems::Main`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum AdvanceWorldSystems {
    /// Runs before [`GgrsSchedule`](`crate::GgrsSchedule`). Use this for setup work that
    /// must happen at the very start of each GGRS frame.
    First,
    /// Despawn any entities if their marked frame has been confirmed.
    /// See [`despawn module docs`](`crate::snapshot::despawn`).
    DespawnConfirmed,
    /// The main GGRS frame step. [`GgrsSchedule`](`crate::GgrsSchedule`) runs here.
    Main,
    /// Runs after [`GgrsSchedule`](`crate::GgrsSchedule`). Use this for teardown or
    /// post-frame work that must happen at the very end of each GGRS frame.
    Last,
}

/// Sets up the [`LoadWorldSystems`] and [`SaveWorldSystems`] sets, allowing for explicit ordering of
/// rollback systems across plugins.
pub struct SnapshotSetPlugin;

impl Plugin for SnapshotSetPlugin {
    /// Configures the system sets for [`LoadWorld`], [`SaveWorld`], and [`AdvanceWorld`]
    /// and inserts [`ApplyDeferred`] barriers between each adjacent pair.
    fn build(&self, app: &mut App) {
        app.init_resource::<StagedInserts>();
        app.configure_sets(
            LoadWorld,
            (
                LoadWorldSystems::EntityResurrect,
                LoadWorldSystems::Entity,
                LoadWorldSystems::EntityFlush,
                LoadWorldSystems::ComponentData,
                LoadWorldSystems::ComponentDataFlush,
                LoadWorldSystems::ResourceData,
                LoadWorldSystems::ResourceDataFlush,
                LoadWorldSystems::Mapping,
            )
                .chain(),
        )
        .configure_sets(
            SaveWorld,
            (SaveWorldSystems::Checksum, SaveWorldSystems::Snapshot).chain(),
        )
        .configure_sets(
            AdvanceWorld,
            (
                AdvanceWorldSystems::First,
                AdvanceWorldSystems::DespawnConfirmed,
                AdvanceWorldSystems::Main,
                AdvanceWorldSystems::Last,
            )
                .chain(),
        )
        .add_systems(
            LoadWorld,
            ApplyDeferred.in_set(LoadWorldSystems::EntityFlush),
        )
        .add_systems(
            LoadWorld,
            (ApplyDeferred, insert_staged_components)
                .chain()
                .in_set(LoadWorldSystems::ComponentDataFlush),
        )
        .add_systems(
            LoadWorld,
            ApplyDeferred.in_set(LoadWorldSystems::ResourceDataFlush),
        )
        .add_systems(
            AdvanceWorld,
            ApplyDeferred
                .after(AdvanceWorldSystems::First)
                .before(AdvanceWorldSystems::Main),
        )
        .add_systems(
            AdvanceWorld,
            ApplyDeferred
                .after(AdvanceWorldSystems::Main)
                .before(AdvanceWorldSystems::Last),
        );
    }
}

//! System set definitions for the [`LoadWorld`], [`SaveWorld`], and [`AdvanceWorld`] schedules.
//!
//! These sets provide explicit ordering hooks so that plugins can interleave their systems
//! cleanly within the rollback loop. See [`LoadWorldSystems`], [`SaveWorldSystems`], and
//! [`AdvanceWorldSystems`] for the available sets and their documented order guarantees.

use bevy::prelude::*;

use crate::snapshot::{AdvanceWorld, LoadWorld, SaveWorld};

/// Set for ordering systems during the [`LoadWorld`] schedule.
/// The most common option is [`LoadWorldSystems::Data`], which is where [`Component`]
/// and [`Resource`] snapshots are loaded and applied to the [`World`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum LoadWorldSystems {
    /// Recreate the [`Entity`] graph as it was during the frame to be rolled back to.
    /// When this set is complete, all entities that were alive during the snapshot
    /// frame have been recreated, and any that were not have been removed. If the
    /// [`Entity`] has changed ID, the new ID will be recorded. All this information will be
    /// made available in [`RollbackEntityMap`](`crate::RollbackEntityMap`).
    Entity,
    /// Flush any deferred operations
    EntityFlush,
    /// Recreate the stored information as it was during the frame to be rolled back to.
    /// When this set is complete, all [`Components`](`Component`) and [`Resources`](`Resource`)
    /// will be rolled back to their exact state during the snapshot.
    ///
    /// NOTE: At this point, [`Entity`] relationships may be broken, see [`LoadWorldSystems::Mapping`]
    /// for when those relationships are fixed.
    Data,
    /// Flush any deferred operations
    DataFlush,
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
        app.configure_sets(
            LoadWorld,
            (
                LoadWorldSystems::Entity,
                LoadWorldSystems::EntityFlush,
                LoadWorldSystems::Data,
                LoadWorldSystems::DataFlush,
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
                AdvanceWorldSystems::Main,
                AdvanceWorldSystems::Last,
            )
                .chain(),
        )
        .add_systems(
            LoadWorld,
            ApplyDeferred.in_set(LoadWorldSystems::EntityFlush),
        )
        .add_systems(LoadWorld, ApplyDeferred.in_set(LoadWorldSystems::DataFlush))
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

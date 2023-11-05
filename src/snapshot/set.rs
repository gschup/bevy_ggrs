use bevy::prelude::*;

use crate::{AdvanceWorld, GgrsSchedule, LoadWorld, SaveWorld};

/// Set for ordering systems during the [`LoadWorld`] schedule.
/// The most common option is [`LoadWorldSet::Data`], which is where [`Component`]
/// and [`Resource`] snapshots are loaded and applied to the [`World`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum LoadWorldSet {
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
    /// NOTE: At this point, [`Entity`] relationships may be broken, see [`LoadWorldSet::Mapping`]
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

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum SaveWorldSet {
    /// Generate checksums for any tracked data.
    ///
    /// Within this set, it is expected that all data which will participate in the
    /// total checksum recorded for this frame will have updated/created a single [`Entity`]
    /// with a [`ChecksumPart`](`crate::ChecksumPart`) component, containing its contribution.
    ///
    /// The final [`Checksum`](`crate::Checksum`) for the frame will be produced after this set, but before
    /// the [`Snapshot`](`SaveWorldSet::Snapshot`) set.
    Checksum,
    /// Saves a snapshot of the [`World`] in this state for future possible rollback.
    Snapshot,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum AdvanceWorldSet {
    First,
    Main,
    Last,
}

/// Sets up the [`LoadWorldSet`] and [`SaveWorldSet`] sets, allowing for explicit ordering of
/// rollback systems across plugins.
pub struct SnapshotSetPlugin;

impl Plugin for SnapshotSetPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            LoadWorld,
            (
                LoadWorldSet::Entity,
                LoadWorldSet::EntityFlush,
                LoadWorldSet::Data,
                LoadWorldSet::DataFlush,
                LoadWorldSet::Mapping,
            )
                .chain(),
        )
        .configure_sets(
            SaveWorld,
            (SaveWorldSet::Checksum, SaveWorldSet::Snapshot).chain(),
        )
        .configure_sets(
            AdvanceWorld,
            (
                AdvanceWorldSet::First,
                AdvanceWorldSet::Main,
                AdvanceWorldSet::Last,
            )
                .chain(),
        )
        .add_systems(LoadWorld, apply_deferred.in_set(LoadWorldSet::EntityFlush))
        .add_systems(LoadWorld, apply_deferred.in_set(LoadWorldSet::DataFlush))
        .add_systems(
            AdvanceWorld,
            apply_deferred
                .after(AdvanceWorldSet::First)
                .before(AdvanceWorldSet::Main),
        )
        .add_systems(
            AdvanceWorld,
            apply_deferred
                .after(AdvanceWorldSet::Main)
                .before(AdvanceWorldSet::Last),
        )
        .add_systems(
            AdvanceWorld,
            (|world: &mut World| world.run_schedule(GgrsSchedule)).in_set(AdvanceWorldSet::Main),
        );
    }
}

use bevy::prelude::*;

use crate::{LoadWorld, SaveWorld};

/// Set for ordering systems during the [`LoadWorld`] schedule.
/// The most common option is [`LoadWorldSet::Data`], which is where [`Component`]
/// and [`Resource`] snapshots are loaded and applied to the [`World`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum LoadWorldSet {
    /// Flush any deferred operations
    PreEntityFlush,
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
    /// Flush any deferred operations
    MappingFlush,
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
    /// Flush any deferred operations
    PreSnapshotFlush,
    /// Saves a snapshot of the [`World`] in this state for future possible rollback.
    Snapshot,
    /// Flush any deferred operations
    PostSnapshotFlush,
}

/// Sets up the [`LoadWorldSet`] and [`SaveWorldSet`] sets, allowing for explicit ordering of
/// rollback systems across plugins.
pub struct GgrsSnapshotSetPlugin;

impl Plugin for GgrsSnapshotSetPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            LoadWorld,
            (
                LoadWorldSet::PreEntityFlush,
                LoadWorldSet::Entity,
                LoadWorldSet::EntityFlush,
                LoadWorldSet::Data,
                LoadWorldSet::DataFlush,
                LoadWorldSet::Mapping,
                LoadWorldSet::MappingFlush,
            )
                .chain(),
        )
        .configure_sets(
            SaveWorld,
            (
                SaveWorldSet::Checksum,
                SaveWorldSet::PreSnapshotFlush,
                SaveWorldSet::Snapshot,
                SaveWorldSet::PostSnapshotFlush,
            )
                .chain(),
        )
        .add_systems(
            LoadWorld,
            apply_deferred.in_set(LoadWorldSet::PreEntityFlush),
        )
        .add_systems(LoadWorld, apply_deferred.in_set(LoadWorldSet::EntityFlush))
        .add_systems(LoadWorld, apply_deferred.in_set(LoadWorldSet::DataFlush))
        .add_systems(LoadWorld, apply_deferred.in_set(LoadWorldSet::MappingFlush))
        .add_systems(
            SaveWorld,
            apply_deferred.in_set(SaveWorldSet::PreSnapshotFlush),
        )
        .add_systems(
            SaveWorld,
            apply_deferred.in_set(SaveWorldSet::PostSnapshotFlush),
        );
    }
}

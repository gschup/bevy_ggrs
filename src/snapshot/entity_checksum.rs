//! Checksum contribution based on the current rollback entity population.
//!
//! [`EntityChecksumPlugin`] hashes the count of active rollback entities and the
//! total number ever spawned into a [`ChecksumPart`], catching desyncs where peers
//! disagree on how many entities exist.

use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::{
    ChecksumFlag, ChecksumPart, RollbackId, RollbackOrdered, SaveWorld, SaveWorldSystems,
    checksum_hasher,
};

/// A plugin that contributes a checksum of the current rollback entity state to the
/// frame checksum.
///
/// It hashes the number of currently active rollback entities and the total number of
/// rollback entities ever spawned. This catches desyncs caused by mismatched entity
/// spawning or despawning across peers.
///
/// Added automatically by [`GgrsPlugin`](`crate::GgrsPlugin`).
pub struct EntityChecksumPlugin;

impl EntityChecksumPlugin {
    /// Computes a [`ChecksumPart`] from entity counts and upserts it into the [`World`].
    #[allow(clippy::type_complexity)]
    pub fn update(
        mut commands: Commands,
        rollback_ordered: Res<RollbackOrdered>,
        active_entities: Query<&RollbackId, (With<RollbackId>, Without<ChecksumFlag<Entity>>)>,
        mut checksum: Query<&mut ChecksumPart, (Without<RollbackId>, With<ChecksumFlag<Entity>>)>,
    ) {
        let mut hasher = checksum_hasher();

        // The quantity of active rollback entities must be synced.
        (active_entities.iter().len() as u64).hash(&mut hasher);

        // The quantity of total spawned rollback entities must be synced.
        (rollback_ordered.len() as u64).hash(&mut hasher);

        let result = ChecksumPart(hasher.finish() as u128);

        trace!("Rollback Entities have checksum {:X}", result.0);

        if let Ok(mut checksum) = checksum.single_mut() {
            *checksum = result;
        } else {
            commands.spawn((result, ChecksumFlag::<Entity>::default()));
        }
    }
}

impl Plugin for EntityChecksumPlugin {
    /// Registers the entity checksum system in [`SaveWorldSystems::Checksum`].
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update.in_set(SaveWorldSystems::Checksum));
    }
}

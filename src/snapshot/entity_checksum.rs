use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::{
    checksum_hasher, ChecksumFlag, ChecksumPart, Rollback, RollbackOrdered, SaveWorld, SaveWorldSet,
};

pub struct EntityChecksumPlugin;

impl EntityChecksumPlugin {
    #[allow(clippy::type_complexity)]
    pub fn update(
        mut commands: Commands,
        rollback_ordered: Res<RollbackOrdered>,
        active_entities: Query<&Rollback, (With<Rollback>, Without<ChecksumFlag<Entity>>)>,
        mut checksum: Query<&mut ChecksumPart, (Without<Rollback>, With<ChecksumFlag<Entity>>)>,
    ) {
        let mut hasher = checksum_hasher();

        // The quantity of active rollback entities must be synced.
        (active_entities.iter().len() as u64).hash(&mut hasher);

        // The quantity of total spawned rollback entities must be synced.
        (rollback_ordered.len() as u64).hash(&mut hasher);

        let result = ChecksumPart(hasher.finish() as u128);

        trace!("Rollback Entities have checksum {:X}", result.0);

        if let Ok(mut checksum) = checksum.get_single_mut() {
            *checksum = result;
        } else {
            commands.spawn((result, ChecksumFlag::<Entity>::default()));
        }
    }
}

impl Plugin for EntityChecksumPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update.in_set(SaveWorldSet::Checksum));
    }
}

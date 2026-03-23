//! Entity identity mapping produced during rollback.
//!
//! When [`EntitySnapshotPlugin`](`super::EntitySnapshotPlugin`) reconstructs the entity
//! graph for a rollback frame, entities that had to be respawned may receive new
//! [`Entity`] IDs. [`RollbackEntityMap`] records the old-to-new mapping so that
//! downstream plugins (e.g. [`ComponentMapEntitiesPlugin`](`super::ComponentMapEntitiesPlugin`))
//! can fix up any stale [`Entity`] references stored in components or resources.

use bevy::{ecs::entity::EntityHashMap, prelude::*};

/// A [`Resource`] which provides an entity-to-entity mapping describing how [`Entity`] IDs
/// changed during a rollback.
#[derive(Resource, Default)]
pub struct RollbackEntityMap(EntityHashMap<Entity>);

impl From<EntityHashMap<Entity>> for RollbackEntityMap {
    fn from(value: EntityHashMap<Entity>) -> Self {
        Self(value)
    }
}

impl RollbackEntityMap {
    /// Iterate over all [`Entity`] mappings as `(old, new)`
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        let Self(map) = self;
        map.iter().map(|(&e1, &e2)| (e1, e2))
    }

    /// Get the mapping for a particular [`Entity`], if it exists.
    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let Self(map) = self;
        map.get(&entity).copied()
    }

    /// The quantity of mappings contained.
    pub fn len(&self) -> usize {
        let Self(map) = self;
        map.len()
    }

    /// Returns `true` if there are no mappings contained, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        let Self(map) = self;
        map.is_empty()
    }
}

impl EntityMapper for &RollbackEntityMap {
    fn get_mapped(&mut self, source: Entity) -> Entity {
        self.get(source).unwrap_or(source)
    }

    fn set_mapped(&mut self, _source: Entity, _target: Entity) {}
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::entity::EntityHashMap, prelude::*};

    use super::RollbackEntityMap;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).expect("valid test entity index")
    }

    fn map_from(pairs: &[(u32, u32)]) -> RollbackEntityMap {
        let inner: EntityHashMap<Entity> = pairs
            .iter()
            .map(|&(old, new)| (entity(old), entity(new)))
            .collect();
        RollbackEntityMap::from(inner)
    }

    /// get returns None for an entity not in the map.
    #[test]
    fn get_absent_key_returns_none() {
        let m = map_from(&[(1, 2)]);
        assert_eq!(m.get(entity(99)), None);
    }

    /// EntityMapper::get_mapped returns the source entity unchanged when not in the map.
    /// Components holding stale Entity references must not be corrupted for unmapped entities.
    #[test]
    fn entity_mapper_falls_back_to_source() {
        let m = map_from(&[(5, 6)]);
        let mut mapper = &m;
        assert_eq!(mapper.get_mapped(entity(99)), entity(99));
    }
}

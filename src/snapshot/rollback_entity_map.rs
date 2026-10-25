//! Entity identity mapping produced during rollback.
//!
//! When [`EntitySnapshotPlugin`](`super::EntitySnapshotPlugin`) reconstructs the entity
//! graph for a rollback frame, entities that had to be respawned may receive new
//! [`Entity`] IDs. [`RollbackEntityMap`] records the old-to-new mapping so that
//! downstream plugins (e.g. [`ComponentMapEntitiesPlugin`](`super::ComponentMapEntitiesPlugin`))
//! can fix up any stale [`Entity`] references stored in components or resources.
//!
//! It holds only the entities whose ID changed. An entity that kept its ID maps to
//! itself and is not in it, so a load that respawned nothing leaves it empty, and the
//! mapping plugins then touch nothing.

use bevy::{
    ecs::{
        change_detection::DetectChangesMut,
        entity::{EntityHashMap, MapEntities},
    },
    prelude::*,
};

/// A [`Resource`] which provides an entity-to-entity mapping describing how [`Entity`] IDs
/// changed during a rollback: the [`Entity`] the snapshot holds to the one alive now, for
/// each rollback entity whose ID differs. Every other entity maps to itself.
#[derive(Resource, Default)]
pub struct RollbackEntityMap(EntityHashMap<Entity>);

impl From<EntityHashMap<Entity>> for RollbackEntityMap {
    fn from(value: EntityHashMap<Entity>) -> Self {
        Self(value)
    }
}

impl RollbackEntityMap {
    /// Iterate over the [`Entity`] IDs that changed, as `(old, new)`
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        let Self(map) = self;
        map.iter().map(|(&e1, &e2)| (e1, e2))
    }

    /// The [`Entity`] alive now for `entity`, if its ID changed.
    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let Self(map) = self;
        map.get(&entity).copied()
    }

    /// The quantity of mappings contained.
    pub fn len(&self) -> usize {
        let Self(map) = self;
        map.len()
    }

    /// Returns `true` if no ID changed: every entity maps to itself.
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

/// Maps `value`'s entities through `map`, and marks it changed only when one of them
/// changed: marked, every value would be copied by the next save as if it had moved.
pub(crate) fn map_changed_entities<T: MapEntities>(
    mut value: impl DetectChangesMut<Inner = T>,
    map: &RollbackEntityMap,
) {
    let mut mapper = ChangeMapper {
        map,
        changed: false,
    };
    value.bypass_change_detection().map_entities(&mut mapper);
    if mapper.changed {
        value.set_changed();
    }
}

/// An [`EntityMapper`] over a [`RollbackEntityMap`] that notes whether it mapped any
/// entity to another.
struct ChangeMapper<'a> {
    map: &'a RollbackEntityMap,
    changed: bool,
}

impl EntityMapper for ChangeMapper<'_> {
    fn get_mapped(&mut self, source: Entity) -> Entity {
        match self.map.get(source) {
            Some(target) => {
                self.changed = true;
                target
            }
            None => source,
        }
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

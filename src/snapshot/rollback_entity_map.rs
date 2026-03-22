use bevy::{ecs::entity::EntityHashMap, prelude::*};

/// A [`Resource`] which provides an [`EntityMap`], describing how [`Entities`](`Entity`)
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

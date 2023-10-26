use bevy::{ecs::entity::EntityMap, prelude::*};

/// A [`Resource`] which provides an [`EntityMap`], describing how [`Entities`](`Entity`)
/// changed during a rollback.
#[derive(Resource, Default)]
pub struct RollbackEntityMap(EntityMap);

impl RollbackEntityMap {
    /// Create a new [`RollbackEntityMap`], which can generate [`EntityMaps`](`EntityMap`) as required.
    pub fn new(map: EntityMap) -> Self {
        Self(map)
    }

    /// Generate an owned [`EntityMap`], which can be used concurrently with other systems.
    pub fn generate_map(&self) -> EntityMap {
        let mut map = EntityMap::default();

        for (original, mapped) in self.iter() {
            map.insert(original, mapped);
        }

        map
    }

    /// Iterate over all [`Entity`] mappings as `(old, new)`
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        let Self(map) = self;
        map.iter()
    }

    /// Get the mapping for a particular [`Entity`], if it exists.
    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let Self(map) = self;
        map.get(entity)
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

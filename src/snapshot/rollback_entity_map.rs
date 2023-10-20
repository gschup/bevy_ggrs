use bevy::{ecs::entity::EntityMap, prelude::*};

/// A [`Resource`] which provides an [`EntityMap`], describing how [`Entities`](`Entity`)
/// changed during a rollback.
#[derive(Resource, Default)]
pub struct RollbackEntityMap(EntityMap);

impl RollbackEntityMap {
    pub fn new(map: EntityMap) -> Self {
        Self(map)
    }

    pub fn get_map(&self) -> EntityMap {
        let mut map = EntityMap::default();

        for (original, mapped) in self.iter() {
            map.insert(original, mapped);
        }

        map
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        let Self(map) = self;
        map.iter()
    }

    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let Self(map) = self;
        map.get(entity)
    }

    pub fn len(&self) -> usize {
        let Self(map) = self;
        map.len()
    }

    pub fn is_empty(&self) -> bool {
        let Self(map) = self;
        map.is_empty()
    }
}

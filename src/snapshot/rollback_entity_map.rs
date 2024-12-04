use bevy::{ecs::entity::EntityHashMap, prelude::*, utils::HashMap};

/// A [`Resource`] which provides an [`EntityMap`], describing how [`Entities`](`Entity`)
/// changed during a rollback.
#[derive(Resource, Default)]
pub struct RollbackEntityMap(EntityHashMap<Entity>);

impl From<EntityHashMap<Entity>> for RollbackEntityMap {
    fn from(value: EntityHashMap<Entity>) -> Self {
        Self(value)
    }
}

impl From<HashMap<Entity, Entity>> for RollbackEntityMap {
    fn from(value: HashMap<Entity, Entity>) -> Self {
        Self(value.into_iter().collect())
    }
}

impl RollbackEntityMap {
    /// Create a new [`RollbackEntityMap`], which can generate [`EntityMaps`](`EntityMap`) as required.
    pub fn new(map: HashMap<Entity, Entity>) -> Self {
        map.into()
    }

    /// Generate an owned [`EntityMap`], which can be used concurrently with other systems.
    pub fn generate_map(&self) -> HashMap<Entity, Entity> {
        let mut map = HashMap::<Entity, Entity>::default();

        for (original, mapped) in self.iter() {
            map.insert(original, mapped);
        }

        map
    }

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
    /// Map the provided [`Entity`], or return it unmodified if it does not need to be mapped.
    fn map_entity(&mut self, entity: Entity) -> Entity {
        self.get(entity).unwrap_or(entity)
    }
}

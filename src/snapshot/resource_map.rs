use std::marker::PhantomData;

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::*,
};

use crate::{LoadWorld, LoadWorldSet, RollbackEntityMap};

/// A [`Plugin`] which updates the state of a post-rollback [`Resource`] `R` using [`MapEntities`].
pub struct GgrsResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for GgrsResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> GgrsResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    /// Exclusive system which will apply a [`RollbackEntityMap`] to the [`Resource`] `R`, provided it implements [`MapEntities`].
    pub fn update(world: &mut World) {
        world.resource_scope(|world: &mut World, map: Mut<RollbackEntityMap>| {
            apply_rollback_map_to_resource_inner::<R>(world, map);
        });
    }
}

fn apply_rollback_map_to_resource_inner<R>(world: &mut World, map: Mut<RollbackEntityMap>)
where
    R: Resource + MapEntities,
{
    let mut applied_entity_map = map.generate_map();

    applied_entity_map.world_scope(world, apply_map::<R>);

    // If the entity map is now larger than the set of rollback entities, then dead entities were created.
    // TODO: This workaround is required because the current behavior of `map_all_entities` is to change all entities,
    // creating dead entities instead of leaving them with their original value. If `EntityMapper` behavior changes,
    // then this workaround may no longer be required.
    if applied_entity_map.len() > map.len() {
        // Reverse dead-mappings, no-op correct mappings
        for original in applied_entity_map.keys().collect::<Vec<_>>() {
            let mapped = applied_entity_map.remove(original).unwrap();

            if map.get(original).is_some() {
                // Rollback entity was correctly mapped; no-op
                applied_entity_map.insert(mapped, mapped);
            } else {
                // An untracked bystander was mapped to a dead end; reverse
                applied_entity_map.insert(mapped, original);
            }
        }

        // Map entities a second time, fixing dead entities
        applied_entity_map.world_scope(world, apply_map::<R>);
    }
}

fn apply_map<R: Resource + MapEntities>(world: &mut World, entity_mapper: &mut EntityMapper) {
    if let Some(mut resource) = world.get_resource_mut::<R>() {
        resource.map_entities(entity_mapper);
    }
}

impl<R> Plugin for GgrsResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSet::Mapping));
    }
}

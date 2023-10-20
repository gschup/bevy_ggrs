use std::marker::PhantomData;

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::*,
};

use crate::{LoadWorld, RollbackEntityMap};

#[derive(Default)]
pub struct GgrsComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    _phantom: PhantomData<C>,
}

impl<C> GgrsComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    /// Exclusive system which will apply a [`RollbackEntityMap`] to the [`Component`] `C`, provided it implements [`MapEntities`].
    pub fn update(world: &mut World) {
        world.resource_scope(|world: &mut World, map: Mut<RollbackEntityMap>| {
            apply_rollback_map_to_component_inner::<C>(world, map);
        });
    }
}

fn apply_rollback_map_to_component_inner<C>(world: &mut World, map: Mut<RollbackEntityMap>)
where
    C: Component + MapEntities,
{
    let mut applied_entity_map = map.get_map();

    applied_entity_map.world_scope(world, apply_map::<C>);

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
        applied_entity_map.world_scope(world, apply_map::<C>);
    }
}

fn apply_map<C: Component + MapEntities>(world: &mut World, entity_mapper: &mut EntityMapper) {
    let entities = entity_mapper.get_map().values().collect::<Vec<Entity>>();

    for entity in &entities {
        if let Some(mut component) = world.get_mut::<C>(*entity) {
            component.map_entities(entity_mapper);
        }
    }
}

impl<C> Plugin for GgrsComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update);
    }
}

use bevy::{
    ecs::{entity::EntityMap, reflect::ReflectMapEntities},
    prelude::*,
    reflect::{Reflect, TypeRegistry},
    utils::HashMap,
};
use std::{fmt::Debug, num::Wrapping};

use crate::rollback::Rollback;

/// Maps rollback_ids to entity id+generation. Necessary to track entities over time.
fn rollback_id_map(world: &mut World) -> HashMap<Rollback, Entity> {
    let mut rid_map = HashMap::default();
    let mut query = world.query::<(Entity, &Rollback)>();
    for (entity, rollback) in query.iter(world) {
        assert!(!rid_map.contains_key(rollback));
        rid_map.insert(*rollback, entity);
    }
    rid_map
}

struct RollbackEntity {
    pub entity: Entity,
    pub rollback_id: Rollback,
    pub components: Vec<Box<dyn Reflect>>,
}

impl Default for RollbackEntity {
    fn default() -> Self {
        Self {
            entity: Entity::from_raw(0),
            rollback_id: Rollback::new(Entity::from_raw(0)),
            components: Default::default(),
        }
    }
}

impl Debug for RollbackEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RollbackEntity")
            .field("id", &self.entity)
            .field("generation", &self.entity.generation())
            .field("rollback_id", &self.rollback_id)
            .finish()
    }
}

/// Holds registered components of `Rollback` tagged entities, as well as registered resources to save and load from/to the real bevy world.
/// The `checksum` is the sum of hash-values from all hashable objects. It is a sum for the checksum to be order insensitive. This of course
/// is not the best checksum to ever exist, but it is a starting point.
#[derive(Default)]
pub(crate) struct WorldSnapshot {
    entities: Vec<RollbackEntity>,
    pub resources: Vec<Box<dyn Reflect>>,
    pub checksum: u64,
}

impl WorldSnapshot {
    pub(crate) fn from_world(world: &World, type_registry: &TypeRegistry) -> Self {
        let mut snapshot = WorldSnapshot::default();
        let type_registry = type_registry.read();

        // create a `RollbackEntity` for every entity tagged with rollback
        for archetype in world.archetypes().iter() {
            let entities_offset = snapshot.entities.len();
            for entity in archetype.entities() {
                let entity = entity.entity();
                if let Some(rollback) = world.get::<Rollback>(entity) {
                    snapshot.entities.push(RollbackEntity {
                        entity,
                        rollback_id: *rollback,
                        components: Vec::new(),
                    });
                }
            }

            // fill the component vectors of rollback entities
            for component_id in archetype.components() {
                let reflect_component = world
                    .components()
                    .get_info(component_id)
                    .and_then(|info| type_registry.get(info.type_id().unwrap()))
                    .and_then(|registration| registration.data::<ReflectComponent>());
                if let Some(reflect_component) = reflect_component {
                    for (i, entity) in archetype
                        .entities()
                        .iter()
                        .filter(|&entity| world.get::<Rollback>(entity.entity()).is_some())
                        .enumerate()
                    {
                        let entity = entity.entity();
                        let entity_ref = world.entity(entity);
                        if let Some(component) = reflect_component.reflect(entity_ref) {
                            assert_eq!(entity, snapshot.entities[entities_offset + i].entity);
                            // add the hash value of that component to the shapshot checksum, if that component supports hashing
                            if let Some(hash) = component.reflect_hash() {
                                // wrapping semantics to avoid overflow
                                snapshot.checksum =
                                    (Wrapping(snapshot.checksum) + Wrapping(hash)).0;
                            }
                            // add the component to the shapshot
                            snapshot.entities[entities_offset + i]
                                .components
                                .push(component.clone_value());
                        }
                    }
                }
            }
        }

        // go through all resources and clone those that are registered
        for (component_id, _) in world.storages().resources.iter() {
            let reflect_component = world
                .components()
                .get_info(component_id)
                .and_then(|info| type_registry.get(info.type_id().unwrap()))
                .and_then(|registration| registration.data::<ReflectResource>());
            if let Some(reflect_resource) = reflect_component {
                if let Some(resource) = reflect_resource.reflect(world) {
                    // add the hash value of that resource to the shapshot checksum, if that resource supports hashing
                    if let Some(hash) = resource.reflect_hash() {
                        snapshot.checksum = (Wrapping(snapshot.checksum) + Wrapping(hash)).0;
                    }
                    // add the resource to the shapshot
                    snapshot.resources.push(resource.clone_value());
                }
            }
        }

        snapshot
    }

    pub(crate) fn write_to_world(&self, world: &mut World, type_registry: &TypeRegistry) {
        let type_registry = type_registry.read();
        let mut rid_map = rollback_id_map(world);

        // Mapping of the old entity ids ( when snapshot was taken ) to new entity ids
        let mut entity_map = EntityMap::default();

        // first, we write all entities
        for rollback_entity in self.entities.iter() {
            // find the corresponding current entity or create new entity, if it doesn't exist
            let entity = *rid_map
                .entry(rollback_entity.rollback_id)
                .or_insert_with(|| {
                    world
                        .spawn(rollback_entity.rollback_id)
                        .id()
                });

            // Add the mapping from the old entity ID to the new entity ID
            entity_map.insert(rollback_entity.entity, entity);

            // for each registered type, check what we need to do
            for registration in type_registry.iter() {
                let type_id = registration.type_id();
                let Some(reflect_component) = registration.data::<ReflectComponent>() else {
                    continue;
                };

                if world.entity(entity).contains_type_id(type_id) {
                    // the entity in the world has such a component
                    match rollback_entity
                        .components
                        .iter()
                        .find(|comp| comp.type_name() == registration.type_name())
                    {
                        // if we have data saved in the snapshot, overwrite the world
                        Some(component) => {
                            // Note: It's important that we remove and re-insert instead of just
                            // apply().
                            //
                            // For example, an apply() will do an in-place update such that apply an
                            // array to an array will add items to the array instead of completely
                            // replacing the current array with the new one.
                            let mut entity_mut = world.entity_mut(entity);
                            reflect_component.remove(&mut entity_mut);
                            reflect_component.insert(&mut entity_mut, &**component);
                        }
                        // if we don't have any data saved, we need to remove that component from the entity
                        None => {
                            let mut entity_mut = world.entity_mut(entity);
                            reflect_component.remove(&mut entity_mut);
                        }
                    }
                } else {
                    // the entity in the world has no such component
                    if let Some(component) = rollback_entity
                        .components
                        .iter()
                        .find(|comp| comp.type_name() == registration.type_name())
                    {
                        // if we have data saved in the snapshot, add the component to the entity
                        let mut entity_mut = world.entity_mut(entity);
                        reflect_component.insert(&mut entity_mut, &**component);
                    }
                    // if both the snapshot and the world does not have the registered component, we don't need to to anything
                }
            }

            // afterwards, remove the pair from the map (leftover entities will need to be despawned)
            rid_map.remove(&rollback_entity.rollback_id);
        }

        // despawn entities which have a rollback component but where not present in the snapshot
        for (_, v) in rid_map.iter() {
            world.despawn(*v);
        }

        // then, we write all resources
        for registration in type_registry.iter() {
            let reflect_resource = match registration.data::<ReflectResource>() {
                Some(res) => res,
                None => {
                    continue;
                }
            };

            match reflect_resource.reflect(world) {
                // the world has such a resource
                Some(_) => {
                    // check if we have saved such a resource
                    match self
                        .resources
                        .iter()
                        .find(|res| res.type_name() == registration.type_name())
                    {
                        // if both the world and the snapshot has the resource, apply the values
                        Some(snapshot_res) => {
                            reflect_resource.apply(world, &**snapshot_res);
                        }
                        // if only the world has the resource, but it doesn't exist in the snapshot, remove the resource
                        None => reflect_resource.remove(world),
                    }
                }
                // the world does not have this resource
                None => {
                    // if we have saved that resource, add it
                    if let Some(snapshot_res) = self
                        .resources
                        .iter()
                        .find(|res| res.type_name() == registration.type_name())
                    {
                        reflect_resource.insert(world, &**snapshot_res);
                    }
                    // if both the world and the snapshot does not have this resource, do nothing
                }
            }
        }

        // For every type that reflects `MapEntities`, map the entities so that they reference the
        // new IDs after applying the snapshot.
        for registration in type_registry.iter() {
            if let Some(map_entities_reflect) = registration.data::<ReflectMapEntities>() {
                map_entities_reflect
                    .map_entities(world, &entity_map)
                    // This may fail if an entity is not found in the entity map, but that's fine,
                    // because if it's not found in the map, then the entity may remain un-mapped.
                    .ok();
            }
        }
    }
}

use bevy::{
    prelude::*,
    reflect::{Reflect, TypeRegistry},
    utils::HashMap,
};
use std::fmt::Debug;

use crate::Rollback;

fn rollback_id_map(world: &mut World) -> HashMap<u32, Entity> {
    let mut rid_map = HashMap::default();
    let mut query = world.query::<(Entity, &Rollback)>();
    for (entity, rollback) in query.iter(world) {
        assert!(!rid_map.contains_key(&rollback.id));
        rid_map.insert(rollback.id, entity);
    }
    rid_map
}

struct RollbackEntity {
    pub entity: Entity,
    pub rollback_id: u32,
    pub components: Vec<Box<dyn Reflect>>,
}

impl Default for RollbackEntity {
    fn default() -> Self {
        Self {
            entity: Entity::new(0),
            ..Default::default()
        }
    }
}

impl Debug for RollbackEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RollbackEntity")
            .field("id", &self.entity.id())
            .field("generation", &self.entity.generation())
            .field("rollback_id", &self.rollback_id)
            .finish()
    }
}

#[derive(Default)]
pub(crate) struct WorldSnapshot {
    entities: Vec<RollbackEntity>,
}

impl WorldSnapshot {
    pub(crate) fn from_world(world: &World, type_registry: &TypeRegistry) -> Self {
        let mut snapshot = WorldSnapshot::default();
        let type_registry = type_registry.read();

        // create a rollback entity for every entity tagged with rollback
        for archetype in world.archetypes().iter() {
            let entities_offset = snapshot.entities.len();
            for entity in archetype.entities() {
                if let Some(rollback) = world.get::<Rollback>(*entity) {
                    snapshot.entities.push(RollbackEntity {
                        entity: *entity,
                        rollback_id: rollback.id,
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
                        .filter(|&&entity| world.get::<Rollback>(entity).is_some())
                        .enumerate()
                    {
                        if let Some(component) = reflect_component.reflect_component(world, *entity)
                        {
                            assert_eq!(*entity, snapshot.entities[entities_offset + i].entity);
                            snapshot.entities[entities_offset + i]
                                .components
                                .push(component.clone_value());
                        }
                    }
                }
            }
        }

        snapshot
    }

    pub(crate) fn write_to_world(&self, world: &mut World, type_registry: &TypeRegistry) {
        let type_registry = type_registry.read();
        let mut rid_map = rollback_id_map(world);

        for rollback_entity in self.entities.iter() {
            // find the corresponding current entity or create new entity, if it doesn't exist
            let entity = *rid_map
                .entry(rollback_entity.rollback_id)
                .or_insert_with(|| {
                    world
                        .spawn()
                        .insert(Rollback {
                            id: rollback_entity.rollback_id,
                        })
                        .id()
                });

            // afterwards, remove the pair from the map (leftover entities will need to be despawned)
            rid_map.remove(&rollback_entity.rollback_id);

            // set the components for that entity
            for component in rollback_entity.components.iter() {
                let registration = type_registry
                    .get_with_name(component.type_name())
                    .expect("Unregistered Type in GGRS Type Registry");
                let reflect_component = registration
                    .data::<ReflectComponent>()
                    .expect("Unregistered Type in GGRS Type Registry");

                // if the entity already has such a component, overwrite it, otherwise add it
                if world
                    .entity(entity)
                    .contains_type_id(registration.type_id())
                {
                    reflect_component.apply_component(world, entity, &**component);
                } else {
                    reflect_component.add_component(world, entity, &**component);
                }
            }
        }

        // despawn entities which have a rollback component but where not present in the snapshot
        for (_, v) in rid_map.iter() {
            world.despawn(*v);
        }
    }
}

use std::marker::PhantomData;

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::*,
};

use crate::{LoadWorld, LoadWorldSet, RollbackEntityMap};

/// A [`Plugin`] which updates the state of a post-rollback [`Resource`] `R` using [`MapEntities`].
///
/// # Examples
/// ```rust
/// # use bevy::{prelude::*, ecs::entity::{MapEntities, EntityMapper}};
/// # use bevy_ggrs::{prelude::*, ResourceMapEntitiesPlugin};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// #[derive(Resource, Clone)]
/// struct Player(Entity);
///
/// impl MapEntities for Player {
///     fn map_entities(&mut self, entity_mapper: &mut EntityMapper) {
///         self.0 = entity_mapper.get_or_reserve(self.0);
///     }
/// }
///
/// // Mapped resources must be snapshot using any supported method
/// app.rollback_resource_with_clone::<Player>();
///
/// // This will apply MapEntities on each rollback
/// app.add_plugins(ResourceMapEntitiesPlugin::<Player>::default());
/// # }
/// ```
pub struct ResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for ResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> ResourceMapEntitiesPlugin<R>
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

    EntityMapper::world_scope(&mut applied_entity_map, world, apply_map::<R>);

    trace!(
        "Mapped {}",
        bevy::utils::get_short_name(std::any::type_name::<R>())
    );

    // If the entity map is now larger than the set of rollback entities, then dead entities were created.
    // TODO: This workaround is required because the current behavior of `map_all_entities` is to change all entities,
    // creating dead entities instead of leaving them with their original value. If `EntityMapper` behavior changes,
    // then this workaround may no longer be required.
    if applied_entity_map.len() > map.len() {
        // Reverse dead-mappings, no-op correct mappings
        for original in applied_entity_map.keys().copied().collect::<Vec<_>>() {
            let mapped = applied_entity_map.remove(&original).unwrap();

            if map.get(original).is_some() {
                // Rollback entity was correctly mapped; no-op
                applied_entity_map.insert(mapped, mapped);
            } else {
                // An untracked bystander was mapped to a dead end; reverse
                applied_entity_map.insert(mapped, original);
            }
        }

        // Map entities a second time, fixing dead entities
        EntityMapper::world_scope(&mut applied_entity_map, world, apply_map::<R>);

        trace!(
            "Re-Mapped {}",
            bevy::utils::get_short_name(std::any::type_name::<R>())
        );
    }
}

fn apply_map<R: Resource + MapEntities>(world: &mut World, entity_mapper: &mut EntityMapper) {
    if let Some(mut resource) = world.get_resource_mut::<R>() {
        resource.map_entities(entity_mapper);
    }
}

impl<R> Plugin for ResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSet::Mapping));
    }
}

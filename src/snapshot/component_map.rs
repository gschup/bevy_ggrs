use std::marker::PhantomData;

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::*,
};

use crate::{LoadWorld, LoadWorldSet, RollbackEntityMap};

/// A [`Plugin`] which updates the state of a post-rollback [`Component`] `C` using [`MapEntities`].
///
/// # Examples
/// ```rust
/// # use bevy::{prelude::*, ecs::entity::{MapEntities, EntityMapper}};
/// # use bevy_ggrs::{prelude::*, ComponentMapEntitiesPlugin};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// #[derive(Component, Clone)]
/// struct BestFriend(Entity);
///
/// impl MapEntities for BestFriend {
///     fn map_entities(&mut self, entity_mapper: &mut EntityMapper) {
///         self.0 = entity_mapper.get_or_reserve(self.0);
///     }
/// }
///
/// // Mapped components must be snapshot using any supported method
/// app.rollback_component_with_clone::<BestFriend>();
///
/// // This will apply MapEntities on each rollback
/// app.add_plugins(ComponentMapEntitiesPlugin::<BestFriend>::default());
/// # }
/// ```
pub struct ComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for ComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<C> ComponentMapEntitiesPlugin<C>
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
    let mut applied_entity_map = map.generate_map();

    EntityMapper::world_scope(&mut applied_entity_map, world, apply_map::<C>);

    trace!(
        "Mapped {}",
        bevy::utils::get_short_name(std::any::type_name::<C>())
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
        EntityMapper::world_scope(&mut applied_entity_map, world, apply_map::<C>);

        trace!(
            "Re-Mapped {}",
            bevy::utils::get_short_name(std::any::type_name::<C>())
        );
    }
}

fn apply_map<C: Component + MapEntities>(world: &mut World, entity_mapper: &mut EntityMapper) {
    let entities = entity_mapper
        .get_map()
        .values()
        .copied()
        .collect::<Vec<Entity>>();

    for entity in &entities {
        if let Some(mut component) = world.get_mut::<C>(*entity) {
            component.map_entities(entity_mapper);
        }
    }
}

impl<C> Plugin for ComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSet::Mapping));
    }
}

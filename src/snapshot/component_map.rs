use std::marker::PhantomData;

use bevy::{ecs::entity::MapEntities, prelude::*};

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
///     fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
///         self.0 = entity_mapper.map_entity(self.0);
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
    for (original, _new) in map.iter() {
        if let Some(mut component) = world.get_mut::<C>(original) {
            component.map_entities(&mut map.as_ref());
        }
    }

    trace!(
        "Mapped {}",
        bevy::utils::get_short_name(std::any::type_name::<C>())
    );
}

impl<C> Plugin for ComponentMapEntitiesPlugin<C>
where
    C: Component + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSet::Mapping));
    }
}

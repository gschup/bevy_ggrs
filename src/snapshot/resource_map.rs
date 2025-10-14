use std::marker::PhantomData;

use bevy::{ecs::entity::MapEntities, prelude::*};

use crate::{LoadWorld, LoadWorldSystems, RollbackEntityMap};

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
///     fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
///         self.0 = entity_mapper.get_mapped(self.0);
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
    if let Some(mut resource) = world.get_resource_mut::<R>() {
        resource.map_entities(&mut map.as_ref());
    }

    trace!("Mapped {}", disqualified::ShortName::of::<R>());
}

impl<R> Plugin for ResourceMapEntitiesPlugin<R>
where
    R: Resource + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSystems::Mapping));
    }
}

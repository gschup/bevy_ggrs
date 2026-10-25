//! Post-rollback entity remapping for [`Resource`] types that hold [`Entity`] references.
//!
//! After a rollback, some entities may have been recreated with new [`Entity`] IDs.
//! [`ResourceMapEntitiesPlugin`] applies the [`RollbackEntityMap`] to any resource that
//! implements [`MapEntities`], keeping cross-entity references inside resources valid.

use std::marker::PhantomData;

use bevy::{
    ecs::{component::Mutable, entity::MapEntities},
    prelude::*,
};

use crate::{
    LoadWorld, LoadWorldSystems, RollbackEntityMap,
    snapshot::rollback_entity_map::map_changed_entities,
};

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
    R: Resource<Mutability = Mutable> + MapEntities,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for ResourceMapEntitiesPlugin<R>
where
    R: Resource<Mutability = Mutable> + MapEntities,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> ResourceMapEntitiesPlugin<R>
where
    R: Resource<Mutability = Mutable> + MapEntities,
{
    /// System which will apply a [`RollbackEntityMap`] to the [`Resource`] `R`, provided it
    /// implements [`MapEntities`]. With no entity ID changed it does nothing, and it marks
    /// the resource changed only when it remapped it.
    pub fn update(map: Res<RollbackEntityMap>, resource: Option<ResMut<R>>) {
        if map.is_empty() {
            return;
        }
        if let Some(resource) = resource {
            map_changed_entities(resource, &map);
        }

        trace!("Mapped {}", disqualified::ShortName::of::<R>());
    }
}

impl<R> Plugin for ResourceMapEntitiesPlugin<R>
where
    R: Resource<Mutability = Mutable> + MapEntities,
{
    /// Registers the entity-mapping system for this resource type in [`LoadWorldSystems::Mapping`].
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSystems::Mapping));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::entity::MapEntities;

    use crate::snapshot::{
        Rollback, RollbackApp, SnapshotPlugin,
        tests::{load_world, save_world},
    };

    use super::*;

    #[derive(Resource, MapEntities, Clone, Copy)]
    struct Target(#[entities] Entity);

    #[derive(Component, Clone, Copy)]
    #[require(Rollback)]
    struct Tracked;

    fn mapping_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        app.rollback_component_with_copy::<Tracked>();
        app.rollback_resource_with_copy::<Target>();
        app.update_resource_with_map_entities::<Target>();
        app.update();
        app
    }

    /// A load that respawned the entity a resource refers to points it at the respawn.
    #[test]
    fn a_respawn_is_mapped_into_the_resource() {
        let mut app = mapping_app();
        let world = app.world_mut();
        let tracked = world.spawn(Tracked).id();
        world.insert_resource(Target(tracked));
        world.flush();

        save_world(world);
        world.despawn(tracked);
        load_world(world, 0);

        let respawned = world
            .query_filtered::<Entity, With<Tracked>>()
            .single(world)
            .unwrap();
        assert_ne!(respawned, tracked);
        assert_eq!(world.resource::<Target>().0, respawned);
    }
}

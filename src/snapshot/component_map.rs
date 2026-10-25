//! Post-rollback entity remapping for [`Component`] types that hold [`Entity`] references.
//!
//! After a rollback, some entities may have been recreated with new [`Entity`] IDs.
//! [`ComponentMapEntitiesPlugin`] applies the [`RollbackEntityMap`] to any component
//! that implements [`MapEntities`], keeping cross-entity references valid.

use std::marker::PhantomData;

use bevy::{
    ecs::{component::Mutable, entity::MapEntities},
    prelude::*,
};

use crate::{
    LoadWorld, LoadWorldSystems, RollbackEntityMap, RollbackId,
    snapshot::rollback_entity_map::map_changed_entities,
};

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
///         self.0 = entity_mapper.get_mapped(self.0);
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
    C: Component<Mutability = Mutable> + MapEntities,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for ComponentMapEntitiesPlugin<C>
where
    C: Component<Mutability = Mutable> + MapEntities,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<C> ComponentMapEntitiesPlugin<C>
where
    C: Component<Mutability = Mutable> + MapEntities,
{
    /// System which will apply a [`RollbackEntityMap`] to the [`Component`] `C` of every
    /// rollback entity, provided it implements [`MapEntities`]. With no entity ID changed
    /// it does nothing, and it marks changed only the components it remapped.
    pub fn update(map: Res<RollbackEntityMap>, mut query: Query<&mut C, With<RollbackId>>) {
        if map.is_empty() {
            return;
        }
        for component in &mut query {
            map_changed_entities(component, &map);
        }

        trace!("Mapped {}", disqualified::ShortName::of::<C>());
    }
}

impl<C> Plugin for ComponentMapEntitiesPlugin<C>
where
    C: Component<Mutability = Mutable> + MapEntities,
{
    /// Registers the entity-mapping system for this component type in [`LoadWorldSystems::Mapping`].
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSystems::Mapping));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{
        AdvanceWorld, Rollback, RollbackApp, SnapshotPlugin,
        tests::{advance_frame, load_world, save_world},
    };
    use bevy::log::LogPlugin;

    #[derive(Resource, Default)]
    enum Input {
        #[default]
        None,
        SpawnFriend,
        DespawnFriends,
    }

    #[derive(Component, MapEntities, Clone, Copy)]
    struct Likes(#[entities] Entity);

    #[derive(Component, Clone, Copy)]
    #[require(Rollback)]
    struct Friend;

    #[derive(Component)]
    #[require(Rollback)]
    struct Player;

    fn like_single_friend(
        mut commands: Commands,
        player: Single<Entity, With<Player>>,
        friends: Query<Entity, With<Friend>>,
    ) {
        // check if there is one and only one friend
        if let Ok(friend) = friends.single() {
            commands.entity(player.entity()).insert(Likes(friend));
        } else {
            // if there are no friends, remove the Likes component from the player
            commands.entity(player.entity()).remove::<Likes>();
        }
    }

    fn spawn_friend(mut commands: Commands, inputs: Res<Input>) {
        if let Input::SpawnFriend = *inputs {
            commands.spawn(Friend);
        }
    }

    fn despawn_friends(
        inputs: Res<Input>,
        friends: Query<Entity, With<Friend>>,
        mut commands: Commands,
    ) {
        if let Input::DespawnFriends = *inputs {
            for friend in &friends {
                commands.entity(friend).despawn();
            }
        }
    }

    fn spawn_player(mut commands: Commands) {
        commands.spawn(Player);
    }

    #[test]
    fn test_map_entities() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        app.rollback_component_with_copy::<Likes>();
        app.update_component_with_map_entities::<Likes>();
        app.add_systems(AdvanceWorld, (spawn_friend, like_single_friend).chain());
        app.add_systems(Startup, spawn_player);
        app.update();

        let get_friend_entity = |world: &mut World| {
            world
                .query_filtered::<Entity, With<Friend>>()
                .single(world)
                .ok()
        };

        let get_liked_entity = |world: &mut World| {
            world
                .query::<&Likes>()
                .single(world)
                .ok()
                .map(|likes| likes.0)
        };

        save_world(app.world_mut()); // save frame 0

        assert_eq!(get_friend_entity(app.world_mut()), None);
        assert_eq!(get_liked_entity(app.world_mut()), None);

        // advance to frame 1, spawns a friend
        app.world_mut().insert_resource(Input::SpawnFriend);
        advance_frame(app.world_mut());

        let initial_friend_entity = get_friend_entity(app.world_mut()).unwrap();
        let initial_liked_entity = get_liked_entity(app.world_mut()).unwrap();
        assert_eq!(initial_friend_entity, initial_liked_entity);

        // roll back to frame 0
        load_world(app.world_mut(), 0);

        assert_eq!(get_friend_entity(app.world_mut()), None);
        assert_eq!(get_liked_entity(app.world_mut()), None);

        // advance to frame 1 again, spawns a friend (a new entity, though)
        app.world_mut().insert_resource(Input::SpawnFriend);
        advance_frame(app.world_mut());

        {
            let friend_entity = get_friend_entity(app.world_mut()).unwrap();
            let liked_entity = get_liked_entity(app.world_mut()).unwrap();
            assert_eq!(friend_entity, liked_entity);
            assert_ne!(friend_entity, initial_friend_entity);
            assert_ne!(liked_entity, initial_liked_entity);
        }
    }

    #[test]
    fn restores_entity_mappings_after_despawning() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(LogPlugin::default());
        app.add_plugins(SnapshotPlugin);
        app.rollback_component_with_copy::<Friend>();
        app.rollback_component_with_copy::<Likes>();
        app.update_component_with_map_entities::<Likes>();
        app.add_systems(
            AdvanceWorld,
            (spawn_friend, like_single_friend, despawn_friends).chain(),
        );
        app.add_systems(Startup, spawn_player);
        app.update();

        let get_friend_entity = |world: &mut World| {
            world
                .query_filtered::<Entity, With<Friend>>()
                .single(world)
                .ok()
        };

        let get_liked_entity = |world: &mut World| {
            world
                .query::<&Likes>()
                .single(world)
                .ok()
                .map(|likes| likes.0)
        };

        // advance to frame 1, spawns a friend
        app.world_mut().insert_resource(Input::SpawnFriend);
        advance_frame(app.world_mut());

        let initial_friend_entity = get_friend_entity(app.world_mut()).unwrap();
        let initial_liked_entity = get_liked_entity(app.world_mut()).unwrap();
        assert_eq!(initial_friend_entity, initial_liked_entity);

        save_world(app.world_mut()); // save frame 1

        // advance to frame 2, despawns a friend
        app.world_mut().insert_resource(Input::DespawnFriends);
        advance_frame(app.world_mut());

        assert_eq!(get_friend_entity(app.world_mut()), None);

        // roll back to frame 1, should restore the friend entity
        load_world(app.world_mut(), 1);

        {
            let friend_entity = get_friend_entity(app.world_mut()).unwrap();
            let liked_entity = get_liked_entity(app.world_mut()).unwrap();
            assert_eq!(friend_entity, liked_entity);
            assert_ne!(friend_entity, initial_friend_entity);
            assert_ne!(liked_entity, initial_liked_entity);
        }
    }

    #[derive(Component, Clone, Copy)]
    #[require(Rollback)]
    struct Tag(u8);

    fn mapping_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        app.rollback_component_with_copy::<Tag>();
        app.rollback_component_with_copy::<Likes>();
        app.update_component_with_map_entities::<Likes>();
        app.update();
        app
    }

    fn tagged(world: &mut World, tag: u8) -> Entity {
        world
            .query::<(Entity, &Tag)>()
            .iter(world)
            .find(|(_, t)| t.0 == tag)
            .map(|(entity, _)| entity)
            .expect("an entity with that tag")
    }

    /// A load that despawned and respawned nothing maps no entity, and leaves every
    /// mapped component as unchanged as the component load left it.
    #[test]
    fn a_load_that_respawns_nothing_maps_nothing() {
        let mut app = mapping_app();
        let world = app.world_mut();
        let friend = world.spawn(Tag(1)).id();
        let player = world.spawn((Tag(0), Likes(friend))).id();
        world.flush();

        save_world(world);
        let before = world.entity(player).get_change_ticks::<Likes>().unwrap();
        load_world(world, 0);

        assert!(world.resource::<RollbackEntityMap>().is_empty());
        let after = world.entity(player).get_change_ticks::<Likes>().unwrap();
        assert_eq!(after.changed, before.changed, "Likes marked changed");
        assert_eq!(world.get::<Likes>(player).unwrap().0, friend);
    }

    /// Respawned entities that refer to each other refer to each other's new IDs.
    #[test]
    fn a_respawned_entity_s_own_references_are_mapped() {
        let mut app = mapping_app();
        let world = app.world_mut();
        let a = world.spawn(Tag(1)).id();
        let b = world.spawn((Tag(2), Likes(a))).id();
        world.entity_mut(a).insert(Likes(b));
        world.flush();

        save_world(world);
        world.despawn(a);
        world.despawn(b);
        load_world(world, 0);

        let (a, b) = (tagged(world, 1), tagged(world, 2));
        assert_eq!(world.get::<Likes>(a).unwrap().0, b);
        assert_eq!(world.get::<Likes>(b).unwrap().0, a);
    }

    /// An entity respawned by one load is alive under a new ID when a second load goes
    /// back to the same frame, whose snapshot holds its old one: references to the old
    /// ID map to the live one.
    #[test]
    fn a_second_load_maps_the_id_a_first_load_respawned_under() {
        let mut app = mapping_app();
        let world = app.world_mut();
        let friend = world.spawn(Tag(1)).id();
        let player = world.spawn((Tag(0), Likes(friend))).id();
        world.flush();

        save_world(world);
        world.despawn(friend);
        load_world(world, 0);
        let respawned = tagged(world, 1);
        assert_ne!(respawned, friend);
        assert_eq!(world.get::<Likes>(player).unwrap().0, respawned);

        load_world(world, 0);
        assert_eq!(tagged(world, 1), respawned);
        assert_eq!(world.get::<Likes>(player).unwrap().0, respawned);
    }
}

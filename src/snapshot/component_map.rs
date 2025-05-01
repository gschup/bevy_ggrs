use std::marker::PhantomData;

use bevy::{
    ecs::{component::Mutable, entity::MapEntities},
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
    /// Exclusive system which will apply a [`RollbackEntityMap`] to the [`Component`] `C`, provided it implements [`MapEntities`].
    pub fn update(world: &mut World) {
        world.resource_scope(|world: &mut World, map: Mut<RollbackEntityMap>| {
            apply_rollback_map_to_component_inner::<C>(world, map);
        });
    }
}

fn apply_rollback_map_to_component_inner<C>(world: &mut World, map: Mut<RollbackEntityMap>)
where
    C: Component<Mutability = Mutable> + MapEntities,
{
    for (original, _new) in map.iter() {
        if let Some(mut component) = world.get_mut::<C>(original) {
            component.map_entities(&mut map.as_ref());
        }
    }

    trace!("Mapped {}", disqualified::ShortName::of::<C>());
}

impl<C> Plugin for ComponentMapEntitiesPlugin<C>
where
    C: Component<Mutability = Mutable> + MapEntities,
{
    fn build(&self, app: &mut App) {
        app.add_systems(LoadWorld, Self::update.in_set(LoadWorldSet::Mapping));
    }
}

#[cfg(test)]
mod tests {
    use ggrs::{Config, GameStateCell, GgrsRequest, InputStatus};
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{prelude::*, schedule_systems::handle_requests};

    #[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum Input {
        #[default]
        None,
        SpawnFriend,
    }

    struct TestConfig;
    impl Config for TestConfig {
        type Input = Input;
        type State = u8;
        type Address = usize;
    }

    #[derive(Component, MapEntities, Clone, Copy)]
    struct Likes(Entity);

    #[derive(Component)]
    struct Friend;

    #[derive(Component)]
    struct Player;

    fn like_single_friend(
        mut commands: Commands,
        player: Single<Entity, With<Player>>,
        friends: Query<Entity, With<Friend>>,
    ) {
        // check if there is one and only one friend
        if let Ok(friend) = friends.single() {
            commands.entity(player.entity()).insert(Likes(friend));
        }
    }

    fn spawn_friend(mut commands: Commands, inputs: Res<PlayerInputs<TestConfig>>) {
        if inputs[0].0 == Input::SpawnFriend {
            commands.spawn(Friend).add_rollback();
        }
    }

    fn spawn_player(mut commands: Commands) {
        commands.spawn(Player).add_rollback();
    }

    #[test]
    fn test_map_entities() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(GgrsPlugin::<TestConfig>::default());
        app.rollback_component_with_copy::<Likes>();
        app.update_component_with_map_entities::<Likes>();
        app.add_systems(GgrsSchedule, (spawn_friend, like_single_friend).chain());
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

        let cell = GameStateCell::default();

        handle_requests(
            vec![GgrsRequest::<TestConfig>::SaveGameState {
                cell: cell.clone(),
                frame: 0,
            }],
            app.world_mut(),
        );

        assert_eq!(get_friend_entity(app.world_mut()), None);
        assert_eq!(get_liked_entity(app.world_mut()), None);

        // advance to frame 1, spawns a friend
        handle_requests(
            vec![GgrsRequest::<TestConfig>::AdvanceFrame {
                inputs: vec![(Input::SpawnFriend, InputStatus::Confirmed)],
            }],
            app.world_mut(),
        );

        let initial_friend_entity = get_friend_entity(app.world_mut()).unwrap();
        let initial_liked_entity = get_liked_entity(app.world_mut()).unwrap();
        assert_eq!(initial_friend_entity, initial_liked_entity);

        // roll back to frame 0
        handle_requests(
            vec![GgrsRequest::<TestConfig>::LoadGameState { cell, frame: 0 }],
            app.world_mut(),
        );

        assert_eq!(get_friend_entity(app.world_mut()), None);
        assert_eq!(get_liked_entity(app.world_mut()), None);

        // advance to frame 1 again, spawns a friend (a new entity, though)
        handle_requests(
            vec![GgrsRequest::<TestConfig>::AdvanceFrame {
                inputs: vec![(Input::SpawnFriend, InputStatus::Confirmed)],
            }],
            app.world_mut(),
        );

        {
            let friend_entity = get_friend_entity(app.world_mut()).unwrap();
            let liked_entity = get_liked_entity(app.world_mut()).unwrap();
            assert_eq!(friend_entity, liked_entity);
            assert_ne!(friend_entity, initial_friend_entity);
            assert_ne!(liked_entity, initial_liked_entity);
        }
    }
}

use crate::{
    GgrsComponentSnapshots, LoadWorld, LoadWorldSet, Rollback, RollbackFrameCount, SaveWorld,
    SaveWorldSet,
};
use bevy::{ecs::hierarchy::ChildOf, prelude::*};

use super::{GgrsComponentSnapshot, RollbackEntityMap};

/// Specialized snapshotting plugin for [`ChildOf`] components.
///
/// ChildOf cannot use ComponentSnapshotPlugin, because:
/// 1. It is an immutable component
/// 2. It requires entity mapping before insertion
pub struct ChildOfSnapshotPlugin;

impl Plugin for ChildOfSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<ChildOf, ChildOf>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<ChildOf, ChildOf>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

impl ChildOfSnapshotPlugin {
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<ChildOf, ChildOf>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, &ChildOf)>,
    ) {
        let components = query
            .iter()
            .map(|(&rollback, component)| (rollback, component.clone()));

        let snapshot = GgrsComponentSnapshot::new(components);

        trace!(
            "Snapshot {} {} component(s)",
            snapshot.iter().count(),
            disqualified::ShortName::of::<ChildOf>()
        );

        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<ChildOf, ChildOf>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<(Entity, &Rollback, Option<&ChildOf>)>,
        map: Res<RollbackEntityMap>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        for (entity, rollback, component) in query.iter_mut() {
            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(_), None) => {
                    commands.entity(entity).remove::<ChildOf>();
                }
                (_, Some(snapshot)) => {
                    if let Some(parent) = map.get(snapshot.0) {
                        commands.entity(entity).insert(ChildOf(parent));
                    } else {
                        warn!("Parent entity not found in rollback map: {:?}", snapshot);
                    }
                }
                (None, None) => {}
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            snapshot.iter().count(),
            disqualified::ShortName::of::<ChildOf>()
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::{prelude::*, AdvanceWorld, LoadWorld, RollbackFrameCount, SaveWorld};
    use bevy::prelude::*;
    use ggrs::*;
    use serde::{Deserialize, Serialize};

    use crate::PlayerInputs;

    struct TestConfig;
    impl Config for TestConfig {
        type Input = Input;
        type State = u8;
        type Address = usize;
    }

    #[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum Input {
        #[default]
        None,
        SpawnChild,
        DespawnChildren,
    }

    #[derive(Component, Clone, Copy)]
    struct Player;

    fn spawn_child(
        mut commands: Commands,
        inputs: Res<PlayerInputs<TestConfig>>,
        player: Single<Entity, With<Player>>,
    ) {
        if inputs[0].0 == Input::SpawnChild {
            commands.spawn(ChildOf(player.entity())).add_rollback();
        }
    }

    fn despawn_children(
        mut commands: Commands,
        inputs: Res<PlayerInputs<TestConfig>>,
        player_children: Single<&Children, With<Player>>,
    ) {
        if inputs[0].0 == Input::DespawnChildren {
            for child in *player_children {
                commands.entity(*child).despawn();
            }
        }
    }

    fn spawn_player(mut commands: Commands) {
        commands.spawn(Player).add_rollback();
    }

    #[test]
    fn test_hirarchy_preservation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(GgrsPlugin::<TestConfig>::default());
        app.add_systems(GgrsSchedule, (spawn_child, despawn_children).chain());
        app.add_systems(Startup, spawn_player);
        app.update();

        let get_player = |world: &mut World| {
            world
                .query_filtered::<Entity, With<Player>>()
                .single(world)
                .unwrap()
        };

        let get_player_children = |world: &mut World| {
            let Ok(children) = world
                .query_filtered::<&Children, With<Player>>()
                .single(world)
            else {
                return vec![];
            };

            children.into_iter().copied().collect::<Vec<Entity>>()
        };

        let get_child_parent = |world: &mut World| {
            world
                .query::<&ChildOf>()
                .single(world)
                .ok()
                .map(|child_of| child_of.0)
        };

        let save = |world: &mut World| {
            world.run_schedule(SaveWorld);
        };

        let advance_frame = |world: &mut World, input: Input| {
            let mut frame_count = world
                .get_resource_mut::<RollbackFrameCount>()
                .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?");
            frame_count.0 += 1;
            world.insert_resource(PlayerInputs::<TestConfig>(vec![(
                input,
                InputStatus::Predicted,
            )]));
            world.run_schedule(AdvanceWorld);
        };

        let load = |world: &mut World, frame: Frame| {
            world
                .get_resource_mut::<RollbackFrameCount>()
                .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
                .0 = frame;
            world.run_schedule(LoadWorld);
        };

        save(app.world_mut());

        assert_eq!(get_player_children(app.world_mut()), vec![]);

        // advance to frame 1, spawns a child
        advance_frame(app.world_mut(), Input::SpawnChild);
        save(app.world_mut());
        let initial_child_enitity = get_player_children(app.world_mut())[0];
        assert_eq!(get_player_children(app.world_mut()).len(), 1);

        // advance to frame 2, despawns the child
        advance_frame(app.world_mut(), Input::DespawnChildren);
        save(app.world_mut());
        assert_eq!(get_player_children(app.world_mut()).len(), 0);

        // roll back to frame 1
        load(app.world_mut(), 1);

        // check that che child was restored
        assert_eq!(get_player_children(app.world_mut()).len(), 1);
        let child_entity_after_rollback = get_player_children(app.world_mut())[0];
        assert_ne!(initial_child_enitity, child_entity_after_rollback);
        assert_eq!(
            get_player(app.world_mut()),
            get_child_parent(app.world_mut()).unwrap()
        );
    }
}

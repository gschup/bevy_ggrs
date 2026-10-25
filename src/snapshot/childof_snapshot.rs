//! Snapshot and restore of parent–child hierarchy ([`ChildOf`]) for rollback entities.
//!
//! [`ChildOfSnapshotPlugin`] is a specialised replacement for [`ComponentSnapshotPlugin`]
//! because [`ChildOf`] is immutable and stores an [`Entity`] that must be remapped through
//! [`RollbackEntityMap`] after the entity graph is reconstructed.

use crate::{
    ClearSnapshots, GgrsComponentSnapshots, HeldComponentSnapshot, LoadWorld, LoadWorldSystems,
    Restore, RollbackFrameCount, SaveWorld, SaveWorldSystems, stage_insert,
};
use bevy::{
    ecs::{change_detection::CheckChangeTicks, hierarchy::ChildOf, system::SystemChangeTick},
    prelude::*,
};

use super::{RollbackEntityMap, RollbackId};

/// Specialized snapshotting plugin for [`ChildOf`] components.
///
/// [`ChildOf`] cannot use [`ComponentSnapshotPlugin`](`crate::ComponentSnapshotPlugin`) because:
/// 1. It is an immutable component.
/// 2. The stored parent [`Entity`] must be remapped through [`RollbackEntityMap`] at restore time.
///
/// Unlike components that go through [`LoadWorldSystems::Mapping`], the remapping is performed
/// inline during [`LoadWorldSystems::ComponentData`] so that the hierarchy is coherent by the time
/// the mapping stage runs.
pub struct ChildOfSnapshotPlugin;

impl Plugin for ChildOfSnapshotPlugin {
    /// Registers [`ChildOf`] snapshot storage and the save/load systems.
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<ChildOf, ChildOf>>()
            .init_resource::<HeldComponentSnapshot<ChildOf, ChildOf>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<ChildOf, ChildOf>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(
                LoadWorld,
                Self::load.in_set(LoadWorldSystems::ComponentData),
            );
        app.add_observer(
            |_trigger: On<ClearSnapshots>,
             mut snapshots: ResMut<GgrsComponentSnapshots<ChildOf, ChildOf>>,
             mut held: ResMut<HeldComponentSnapshot<ChildOf, ChildOf>>| {
                snapshots.clear();
                held.clear();
            },
        );
        app.add_observer(
            |check: On<CheckChangeTicks>,
             mut held: ResMut<HeldComponentSnapshot<ChildOf, ChildOf>>| {
                held.check_change_ticks(*check);
            },
        );
    }
}

impl ChildOfSnapshotPlugin {
    /// System that snapshots the [`ChildOf`] component on all rollback entities for this frame.
    /// [`ChildOf`] is immutable, so only the chunks where one was inserted or
    /// removed are copied ([`HeldComponentSnapshot`]).
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<ChildOf, ChildOf>>,
        mut held: ResMut<HeldComponentSnapshot<ChildOf, ChildOf>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&RollbackId, Ref<ChildOf>)>,
        changed: Query<Ref<ChildOf>, With<RollbackId>>,
        ticks: SystemChangeTick,
    ) {
        let snapshot = held.save(
            changed.iter().map(|child_of| child_of.last_changed()),
            query
                .iter()
                .map(|(&rollback, child_of)| (rollback, child_of.last_changed())),
            query
                .iter()
                .map(|(&rollback, child_of)| (rollback, child_of)),
            |child_of| ChildOf::clone(&child_of),
            ticks.this_run(),
        );

        trace!(
            "Snapshot {} {} component(s)",
            snapshot.len(),
            disqualified::ShortName::of::<ChildOf>()
        );

        snapshots.push(frame.0, snapshot);
    }

    /// System that restores the [`ChildOf`] component for the target frame, remapping parent
    /// entities through [`RollbackEntityMap`] to account for any ID changes. An entity without
    /// one gets it from [`insert_staged_components`](crate::insert_staged_components), together
    /// with the rest of its staged components.
    ///
    /// A chunk that is the one the world holds is skipped: its children have
    /// the parents the snapshot names, which are alive, so no map entry names
    /// them.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<ChildOf, ChildOf>>,
        mut held: ResMut<HeldComponentSnapshot<ChildOf, ChildOf>>,
        frame: Res<RollbackFrameCount>,
        with: Query<(Entity, &RollbackId, Ref<ChildOf>)>,
        without: Query<(Entity, &RollbackId), Without<ChildOf>>,
        rollback_entities: Query<(), With<RollbackId>>,
        map: Res<RollbackEntityMap>,
        ticks: SystemChangeTick,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();
        let Some(plan) = held.load(
            snapshot,
            with.iter().map(|(_, _, child_of)| child_of.last_changed()),
            with.iter()
                .map(|(_, &rollback, child_of)| (rollback, child_of.last_changed())),
            ticks.this_run(),
        ) else {
            return;
        };
        // The stored parent under the ID it has now.
        let parent_of = |stored: &ChildOf| map.get(stored.0).unwrap_or(stored.0);
        let is_rollback = |parent: Entity| {
            let found = rollback_entities.contains(parent);
            if !found {
                warn!("Parent entity is not a rollback entity: {parent:?}");
            }
            found
        };

        // The children first, then, only when the snapshot holds some the
        // world does not, the rest: as the component loads, so a load walks
        // the entities with a parent, not every rollback entity.
        let mut matched = 0;
        for (entity, rollback, child_of) in &with {
            let stored = match plan.restore(rollback) {
                Restore::Shared => continue,
                Restore::Absent => {
                    commands.entity(entity).remove::<ChildOf>();
                    continue;
                }
                Restore::Stored(stored) => stored,
            };
            matched += 1;
            // Already the child of that parent: nothing to do. A re-insert
            // fires the relationship hooks and rebuilds the parent's
            // `Children`, for every child, every load.
            let parent = parent_of(stored);
            if child_of.parent() != parent && is_rollback(parent) {
                commands.entity(entity).insert(ChildOf(parent));
            }
        }
        if plan.holds_more(matched) {
            for (entity, rollback) in &without {
                let Some(stored) = plan.missing(rollback) else {
                    continue;
                };
                let parent = parent_of(stored);
                if is_rollback(parent) {
                    commands.queue(stage_insert(entity, *rollback, ChildOf(parent)));
                }
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
    use crate::snapshot::{
        AdvanceWorld, Rollback, SnapshotPlugin,
        tests::{advance_frame, load_world, save_world},
    };
    use bevy::prelude::*;

    #[derive(Resource, Default)]
    enum Input {
        #[default]
        None,
        SpawnChild,
        DespawnChildren,
    }

    #[derive(Component, Clone, Copy)]
    #[require(Rollback)]
    struct Player;

    fn spawn_child(
        mut commands: Commands,
        input: Res<Input>,
        player: Single<Entity, With<Player>>,
    ) {
        if let Input::SpawnChild = *input {
            commands.spawn((ChildOf(player.entity()), Rollback));
        }
    }

    fn despawn_children(
        mut commands: Commands,
        input: Res<Input>,
        player_children: Single<&Children, With<Player>>,
    ) {
        if let Input::DespawnChildren = *input {
            for child in *player_children {
                commands.entity(*child).despawn();
            }
        }
    }

    fn spawn_player(mut commands: Commands) {
        commands.spawn(Player);
    }

    #[test]
    fn test_hierarchy_preservation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        app.add_systems(AdvanceWorld, (spawn_child, despawn_children).chain());
        app.add_systems(Startup, spawn_player);
        app.update();

        let advance_with_input = |world: &mut World, input: Input| {
            world.insert_resource(input);
            advance_frame(world);
        };

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

        save_world(app.world_mut());

        assert_eq!(get_player_children(app.world_mut()), vec![]);

        // advance to frame 1, spawns a child
        advance_with_input(app.world_mut(), Input::SpawnChild);
        save_world(app.world_mut());
        let initial_child_enitity = get_player_children(app.world_mut())[0];
        assert_eq!(get_player_children(app.world_mut()).len(), 1);

        // advance to frame 2, despawns the child
        advance_with_input(app.world_mut(), Input::DespawnChildren);
        save_world(app.world_mut());
        assert_eq!(get_player_children(app.world_mut()).len(), 0);

        // roll back to frame 1
        load_world(app.world_mut(), 1);

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

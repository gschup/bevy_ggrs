//! Snapshot and restore of the rollback entity set.
//!
//! [`EntitySnapshotPlugin`] saves a mapping of [`RollbackId`] → [`Entity`] each frame.
//! On rollback, it reconciles the live entity set against the snapshot — spawning
//! missing entities, despawning extras, and recording any ID changes in a
//! [`RollbackEntityMap`] so that subsequent plugins can fix up stale entity references.

use crate::{
    ClearSnapshots, GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSystems,
    Rollback, RollbackEntityMap, RollbackFrameCount, RollbackId, SaveWorld, SaveWorldSystems,
};
use bevy::{ecs::entity::EntityHashMap, platform::collections::HashSet, prelude::*};

/// A [`Plugin`] which manages the rollback for [`Entities`](`Entity`). This will ensure
/// all [`Entities`](`Entity`) match the state of the desired frame, or can be mapped using a
/// [`RollbackEntityMap`], which this [`Plugin`] will also manage.
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, EntitySnapshotPlugin};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// // This will ensure entities are updated on rollback to match the state of the target snapshot
/// app.add_plugins(EntitySnapshotPlugin);
/// # }
/// ```
pub struct EntitySnapshotPlugin;

impl EntitySnapshotPlugin {
    /// System that records the current [`RollbackId`] → [`Entity`] mapping for this frame.
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&RollbackId, Entity)>,
    ) {
        let entities = query.iter().map(|(&rollback, entity)| (rollback, entity));

        let snapshot = GgrsComponentSnapshot::new(entities);

        trace!("Snapshot {} entity(s)", snapshot.iter().count());

        snapshots.push(frame.0, snapshot);
    }

    /// System that reconciles live entities against the snapshot for the target frame,
    /// spawning or despawning as needed and populating [`RollbackEntityMap`] with any ID changes.
    ///
    /// The live entities first, each looked up in the snapshot; then, only when the
    /// snapshot holds some the world does not, the snapshot, for those to respawn. A load
    /// that respawned nothing and found every entity under its own ID leaves the map
    /// empty.
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
        mut map: ResMut<RollbackEntityMap>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&RollbackId, Entity)>,
    ) {
        let mut entity_map = EntityHashMap::<Entity>::default();

        let snapshot = snapshots.rollback(frame.0).get();

        let mut matched = 0;
        for (rollback, current_entity) in &query {
            match snapshot.get(rollback) {
                Some(&old_entity) => {
                    matched += 1;
                    if old_entity != current_entity {
                        entity_map.insert(old_entity, current_entity);
                    }
                }
                None => {
                    // A child the frame lacks along with its parent is gone
                    // already when the parent's despawn came first.
                    commands.entity(current_entity).try_despawn();
                }
            }
        }

        if matched < snapshot.len() {
            let alive: HashSet<RollbackId> = query.iter().map(|(&rollback, _)| rollback).collect();
            for (&rollback, &old_entity) in snapshot.iter() {
                if alive.contains(&rollback) {
                    continue;
                }
                // A respawn is bare: it gets the components registered for rollback and
                // nothing else the entity had. A rollback entity despawned outright
                // while a snapshot still holds it ends here; `despawn_rollback` marks it
                // instead, and the mark lifts whole.
                warn!(
                    "respawning {rollback:?} bare: it was despawned while frame {} still \
                     held it; despawn_rollback keeps every component",
                    frame.0
                );
                let current_entity = commands.spawn((rollback, Rollback)).id();
                entity_map.insert(old_entity, current_entity);
            }
        }

        trace!("Rolled back {} entity(s)", snapshot.len());

        *map = entity_map.into();
    }
}

impl Plugin for EntitySnapshotPlugin {
    /// Registers entity snapshot storage, [`RollbackEntityMap`], and the save/load systems.
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<Entity>>()
            .init_resource::<RollbackEntityMap>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<Entity>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSystems::Entity));
        app.add_observer(
            |_trigger: On<ClearSnapshots>,
             mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>| {
                snapshots.clear();
            },
        );
    }
}

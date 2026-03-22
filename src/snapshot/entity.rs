//! Snapshot and restore of the rollback entity set.
//!
//! [`EntitySnapshotPlugin`] saves a mapping of [`RollbackId`] → [`Entity`] each frame.
//! On rollback, it reconciles the live entity set against the snapshot — spawning
//! missing entities, despawning extras, and recording any ID changes in a
//! [`RollbackEntityMap`] so that subsequent plugins can fix up stale entity references.

use crate::{
    GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSystems, Rollback,
    RollbackEntityMap, RollbackFrameCount, RollbackId, SaveWorld, SaveWorldSystems,
};
use bevy::{ecs::entity::EntityHashMap, platform::collections::HashMap, prelude::*};

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
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
        mut map: ResMut<RollbackEntityMap>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&RollbackId, Entity)>,
    ) {
        let mut entity_map = HashMap::<Entity, Entity>::default();
        let mut rollback_mapping = HashMap::new();

        let snapshot = snapshots.rollback(frame.0).get();

        for (&rollback, &old_entity) in snapshot.iter() {
            rollback_mapping.insert(rollback, (None, Some(old_entity)));
        }

        for (&rollback, current_entity) in query.iter() {
            rollback_mapping.entry(rollback).or_insert((None, None)).0 = Some(current_entity);
        }

        for (rollback, (current_entity, old_entity)) in rollback_mapping {
            match (current_entity, old_entity) {
                (Some(current_entity), Some(old_entity)) => {
                    entity_map.insert(current_entity, old_entity);
                }
                (Some(current_entity), None) => {
                    commands.entity(current_entity).despawn();
                }
                (None, Some(old_entity)) => {
                    let current_entity = commands.spawn((rollback, Rollback)).id();
                    entity_map.insert(old_entity, current_entity);
                }
                (None, None) => unreachable!(
                    "Rollback keys could only be added if they had an old or current Entity"
                ),
            }
        }

        trace!("Rolled back {} entity(s)", snapshot.iter().count());

        *map = entity_map
            .into_iter()
            .collect::<EntityHashMap<Entity>>()
            .into();
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
                    GgrsComponentSnapshots::<Entity>::sync_depth,
                    GgrsComponentSnapshots::<Entity>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSystems::Entity));
    }
}

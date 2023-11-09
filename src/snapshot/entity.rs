use crate::{
    GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSet, Rollback,
    RollbackEntityMap, RollbackFrameCount, SaveWorld, SaveWorldSet,
};
use bevy::{prelude::*, utils::HashMap};

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
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, Entity)>,
    ) {
        let entities = query.iter().map(|(&rollback, entity)| (rollback, entity));

        let snapshot = GgrsComponentSnapshot::new(entities);

        trace!("Snapshot {} entity(s)", snapshot.iter().count());

        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
        mut map: ResMut<RollbackEntityMap>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, Entity)>,
    ) {
        let mut entity_map = HashMap::default();
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
                    let current_entity = commands.spawn(rollback).id();
                    entity_map.insert(old_entity, current_entity);
                }
                (None, None) => unreachable!(
                    "Rollback keys could only be added if they had an old or current Entity"
                ),
            }
        }

        trace!("Rolled back {} entity(s)", snapshot.iter().count());

        *map = RollbackEntityMap::new(entity_map);
    }
}

impl Plugin for EntitySnapshotPlugin {
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
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Entity));
    }
}

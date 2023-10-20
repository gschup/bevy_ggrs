use crate::{
    GgrsComponentSnapshot, GgrsSnapshots, LoadWorld, Rollback, RollbackEntityMap,
    RollbackFrameCount, SaveWorld,
};
use bevy::{ecs::entity::EntityMap, prelude::*, utils::HashMap};

pub struct GgrsEntitySnapshotPlugin;

impl GgrsEntitySnapshotPlugin {
    pub fn save(
        mut snapshots: ResMut<GgrsSnapshots<Entity, GgrsComponentSnapshot<Entity>>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, Entity)>,
    ) {
        let entities = query.iter().map(|(&rollback, entity)| (rollback, entity));
        let snapshot = GgrsComponentSnapshot::new(entities);
        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsSnapshots<Entity, GgrsComponentSnapshot<Entity>>>,
        mut map: ResMut<RollbackEntityMap>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, Entity)>,
    ) {
        let mut entity_map = EntityMap::default();
        let mut rollback_mapping = HashMap::new();

        let snapshot = snapshots.rollback(frame.0).get();

        for (&rollback, &old_entity) in snapshot.iter() {
            rollback_mapping.insert(rollback, (None, Some(old_entity)));
        }

        for (&rollback, current_entity) in query.iter() {
            rollback_mapping.entry(rollback).or_insert((None, None)).0 = Some(current_entity);
        }

        for (current_entity, old_entity) in rollback_mapping.values() {
            match (current_entity, old_entity) {
                (Some(current_entity), Some(old_entity)) => {
                    entity_map.insert(*current_entity, *old_entity);
                }
                (Some(current_entity), None) => {
                    commands.entity(*current_entity).despawn();
                }
                (None, Some(old_entity)) => {
                    let current_entity = commands.spawn_empty().id();
                    entity_map.insert(current_entity, *old_entity);
                }
                (None, None) => unreachable!(
                    "Rollback keys could only be added if they had an old or current Entity"
                ),
            }
        }

        *map = RollbackEntityMap::new(entity_map);
    }
}

impl Plugin for GgrsEntitySnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsSnapshots<Entity, GgrsComponentSnapshot<Entity>>>()
            .init_resource::<RollbackEntityMap>()
            .add_systems(SaveWorld, Self::save)
            .add_systems(LoadWorld, Self::load);
    }
}

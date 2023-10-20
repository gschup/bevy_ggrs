use crate::{
    GgrsComponentSnapshot, GgrsSnapshots, LoadWorld, LoadWorldSet, Rollback, RollbackFrameCount,
    SaveWorld,
};
use bevy::prelude::*;
use std::marker::PhantomData;

pub struct GgrsComponentSnapshotClonePlugin<C>
where
    C: Component + Clone,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for GgrsComponentSnapshotClonePlugin<C>
where
    C: Component + Clone,
{
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<C> GgrsComponentSnapshotClonePlugin<C>
where
    C: Component + Clone,
{
    pub fn save(
        mut snapshots: ResMut<GgrsSnapshots<C, GgrsComponentSnapshot<C>>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, &C)>,
    ) {
        let components = query
            .iter()
            .map(|(&rollback, component)| (rollback, component.clone()));
        let snapshot = GgrsComponentSnapshot::new(components);
        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsSnapshots<C, GgrsComponentSnapshot<C>>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<(Entity, &Rollback, Option<&mut C>)>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        for (entity, rollback, component) in query.iter_mut() {
            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(mut component), Some(snapshot)) => *component = snapshot.clone(),
                (Some(_), None) => {
                    commands.entity(entity).remove::<C>();
                }
                (None, Some(snapshot)) => {
                    commands.entity(entity).insert(snapshot.clone());
                }
                (None, None) => {}
            }
        }
    }
}

impl<C> Plugin for GgrsComponentSnapshotClonePlugin<C>
where
    C: Component + Clone,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsSnapshots<C, GgrsComponentSnapshot<C>>>()
            .add_systems(SaveWorld, Self::save)
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

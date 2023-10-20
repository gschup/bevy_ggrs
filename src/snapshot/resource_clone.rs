use crate::{GgrsSnapshots, LoadWorld, LoadWorldSet, RollbackFrameCount, SaveWorld};
use bevy::prelude::*;
use std::marker::PhantomData;

pub struct GgrsResourceSnapshotClonePlugin<R>
where
    R: Resource + Clone,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for GgrsResourceSnapshotClonePlugin<R>
where
    R: Resource + Clone,
{
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<R> GgrsResourceSnapshotClonePlugin<R>
where
    R: Resource + Clone,
{
    pub fn save(
        mut snapshots: ResMut<GgrsSnapshots<R, Option<R>>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<Res<R>>,
    ) {
        snapshots.push(frame.0, resource.map(|res| res.clone()));
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsSnapshots<R, Option<R>>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<ResMut<R>>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        match (resource, snapshot) {
            (Some(mut resource), Some(snapshot)) => *resource = snapshot.clone(),
            (Some(_), None) => commands.remove_resource::<R>(),
            (None, Some(snapshot)) => commands.insert_resource(snapshot.clone()),
            (None, None) => {}
        }
    }
}

impl<R> Plugin for GgrsResourceSnapshotClonePlugin<R>
where
    R: Resource + Clone,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsSnapshots<R, Option<R>>>()
            .add_systems(SaveWorld, Self::save)
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

use bevy::prelude::*;
use crate::{schedule_systems::{load_world, save_world}, RollbackFrameCount, SaveWorld, LoadWorld, GgrsSnapshots};
use std::marker::PhantomData;

#[derive(Default)]
pub struct GgrsResourceSnapshotClonePlugin<R>
where
    R: Resource + Clone,
{
    _phantom: PhantomData<R>,
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
            .add_systems(SaveWorld, Self::save.after(save_world))
            .add_systems(LoadWorld, Self::load.after(load_world));
    }
}
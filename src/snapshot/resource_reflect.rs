use crate::{
    schedule_systems::{load_world, save_world},
    GgrsSnapshots, LoadWorld, RollbackFrameCount, SaveWorld,
};
use bevy::prelude::*;
use std::marker::PhantomData;

#[derive(Default)]
pub struct GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    _phantom: PhantomData<R>,
}

impl<R> GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    pub fn save(
        mut snapshots: ResMut<GgrsSnapshots<R, Option<Box<dyn Reflect>>>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<Res<R>>,
    ) {
        snapshots.push(frame.0, resource.map(|res| res.as_reflect().clone_value()));
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsSnapshots<R, Option<Box<dyn Reflect>>>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<ResMut<R>>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        match (resource, snapshot) {
            (Some(mut resource), Some(snapshot)) => {
                resource.apply(snapshot.as_ref());
            }
            (Some(_), None) => commands.remove_resource::<R>(),
            (None, Some(snapshot)) => {
                let snapshot = snapshot.clone_value();

                commands.add(move |world: &mut World| {
                    let mut resource = R::from_world(world);
                    resource.apply(snapshot.as_ref());
                    world.insert_resource(resource);
                })
            }
            (None, None) => {}
        }
    }
}

impl<R> Plugin for GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsSnapshots<R, Option<Box<dyn Reflect>>>>()
            .add_systems(SaveWorld, Self::save.after(save_world))
            .add_systems(LoadWorld, Self::load.after(load_world));
    }
}

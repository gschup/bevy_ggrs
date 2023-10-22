use crate::{GgrsSnapshots, LoadWorld, LoadWorldSet, RollbackFrameCount, SaveWorld, SaveWorldSet};
use bevy::prelude::*;
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Resource`] `R` using [`Reflect`] and [`FromWorld`].
///
/// NOTE: [`FromWorld`] is implemented for all types implementing [`Default`].
pub struct GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    _phantom: PhantomData<R>,
}

type Snapshots<R> = GgrsSnapshots<R, Option<Box<dyn Reflect>>>;

impl<R> Default for GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> GgrsResourceSnapshotReflectPlugin<R>
where
    R: Resource + Reflect + FromWorld,
{
    pub fn save(
        mut snapshots: ResMut<Snapshots<R>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<Res<R>>,
    ) {
        snapshots.push(frame.0, resource.map(|res| res.as_reflect().clone_value()));
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<Snapshots<R>>,
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
        app.init_resource::<Snapshots<R>>()
            .add_systems(
                SaveWorld,
                (Snapshots::<R>::discard_old_snapshots, Self::save)
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

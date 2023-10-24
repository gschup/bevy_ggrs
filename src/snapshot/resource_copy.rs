use crate::{
    GgrsResourceSnapshots, LoadWorld, LoadWorldSet, RollbackFrameCount, SaveWorld, SaveWorldSet,
};
use bevy::prelude::*;
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Resource`] `R` using [`Copy`].
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::prelude::*;
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// // A marker is an ideal data type to rollback with Copy
/// #[derive(Resource, Clone, Copy)]
/// struct MyMarker;
/// 
/// app.add_plugins(GgrsResourceSnapshotCopyPlugin::<MyMarker>::default());
/// # }
/// ```
pub struct GgrsResourceSnapshotCopyPlugin<R>
where
    R: Resource + Copy,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for GgrsResourceSnapshotCopyPlugin<R>
where
    R: Resource + Copy,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> GgrsResourceSnapshotCopyPlugin<R>
where
    R: Resource + Copy,
{
    pub fn save(
        mut snapshots: ResMut<GgrsResourceSnapshots<R>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<Res<R>>,
    ) {
        snapshots.push(frame.0, resource.map(|res| *res));

        trace!(
            "Snapshot {}",
            bevy::utils::get_short_name(std::any::type_name::<R>())
        );
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsResourceSnapshots<R>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<ResMut<R>>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        match (resource, snapshot) {
            (Some(mut resource), Some(snapshot)) => *resource = *snapshot,
            (Some(_), None) => commands.remove_resource::<R>(),
            (None, Some(snapshot)) => commands.insert_resource(*snapshot),
            (None, None) => {}
        }

        trace!(
            "Rolled Back {}",
            bevy::utils::get_short_name(std::any::type_name::<R>())
        );
    }
}

impl<R> Plugin for GgrsResourceSnapshotCopyPlugin<R>
where
    R: Resource + Copy,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsResourceSnapshots<R>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsResourceSnapshots::<R>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

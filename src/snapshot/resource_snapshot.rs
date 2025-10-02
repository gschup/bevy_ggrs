use crate::{
    GgrsResourceSnapshots, LoadWorld, LoadWorldSystems, RollbackFrameCount, SaveWorld, SaveWorldSystems,
    Strategy,
};
use bevy::prelude::*;
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Resource`] using a provided [`Strategy`].
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ResourceSnapshotPlugin, CloneStrategy};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// #[derive(Resource, Clone)]
/// struct BossHealth(u32);
///
/// // This will ensure the BossHealth resource is rolled back
/// app.add_plugins(ResourceSnapshotPlugin::<CloneStrategy<BossHealth>>::default());
/// # }
/// ```
pub struct ResourceSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Resource,
    S::Stored: Send + Sync + 'static,
{
    _phantom: PhantomData<S>,
}

impl<S> Default for ResourceSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Resource,
    S::Stored: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<S> ResourceSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Resource,
    S::Stored: Send + Sync + 'static,
{
    pub fn save(
        mut snapshots: ResMut<GgrsResourceSnapshots<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<Res<S::Target>>,
    ) {
        snapshots.push(frame.0, resource.map(|res| S::store(res.as_ref())));

        trace!("Snapshot {}", disqualified::ShortName::of::<S::Target>());
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsResourceSnapshots<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        resource: Option<ResMut<S::Target>>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        match (resource, snapshot) {
            (Some(mut resource), Some(snapshot)) => S::update(resource.as_mut(), snapshot),
            (Some(_), None) => commands.remove_resource::<S::Target>(),
            (None, Some(snapshot)) => commands.insert_resource(S::load(snapshot)),
            (None, None) => {}
        }

        trace!("Rolled back {}", disqualified::ShortName::of::<S::Target>());
    }
}

impl<S> Plugin for ResourceSnapshotPlugin<S>
where
    S: Send + Sync + 'static + Strategy,
    S::Target: Resource,
    S::Stored: Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsResourceSnapshots<S::Target, S::Stored>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsResourceSnapshots::<S::Target, S::Stored>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSystems::Data));
    }
}

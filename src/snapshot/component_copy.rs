use crate::{
    GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSet, Rollback,
    RollbackFrameCount, SaveWorld, SaveWorldSet,
};
use bevy::prelude::*;
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Component`] `C` using [`Copy`].
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
/// // A marker component is an ideal data type to rollback with Copy
/// #[derive(Component, Clone, Copy)]
/// struct MyMarker;
///
/// app.add_plugins(ComponentSnapshotCopyPlugin::<MyMarker>::default());
/// # }
/// ```
pub struct ComponentSnapshotCopyPlugin<C>
where
    C: Component + Copy,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for ComponentSnapshotCopyPlugin<C>
where
    C: Component + Copy,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<C> ComponentSnapshotCopyPlugin<C>
where
    C: Component + Copy,
{
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<C>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, &C)>,
    ) {
        let components = query
            .iter()
            .map(|(&rollback, &component)| (rollback, component));

        let snapshot = GgrsComponentSnapshot::new(components);

        trace!(
            "Snapshot {} {} component(s)",
            snapshot.iter().count(),
            bevy::utils::get_short_name(std::any::type_name::<C>())
        );

        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<C>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<(Entity, &Rollback, Option<&mut C>)>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        for (entity, rollback, component) in query.iter_mut() {
            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(mut component), Some(snapshot)) => *component = *snapshot,
                (Some(_), None) => {
                    commands.entity(entity).remove::<C>();
                }
                (None, Some(snapshot)) => {
                    commands.entity(entity).insert(*snapshot);
                }
                (None, None) => {}
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            snapshot.iter().count(),
            bevy::utils::get_short_name(std::any::type_name::<C>())
        );
    }
}

impl<C> Plugin for ComponentSnapshotCopyPlugin<C>
where
    C: Component + Copy,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<C>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<C>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

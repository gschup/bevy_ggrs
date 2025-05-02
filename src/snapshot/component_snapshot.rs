use crate::{
    GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSet, Rollback,
    RollbackFrameCount, SaveWorld, SaveWorldSet, Strategy,
};
use bevy::{ecs::component::ComponentMutability, prelude::*};
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Component`] using a provided [`Strategy`].
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ComponentSnapshotPlugin, CloneStrategy};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// // The Transform component is a good candidate for Clone-based rollback
/// app.add_plugins(ComponentSnapshotPlugin::<CloneStrategy<Transform>>::default());
/// # }
/// ```
pub struct ComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    _phantom: PhantomData<S>,
}

impl<S> Default for ComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<S> ComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, &S::Target)>,
    ) {
        let components = query
            .iter()
            .map(|(&rollback, component)| (rollback, S::store(component)));

        let snapshot = GgrsComponentSnapshot::new(components);

        trace!(
            "Snapshot {} {} component(s)",
            snapshot.iter().count(),
            disqualified::ShortName::of::<S::Target>()
        );

        snapshots.push(frame.0, snapshot);
    }
}

impl<S> ComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<EntityMut, With<Rollback>>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        for mut entity in query.iter_mut() {
            let (rollback, component) = entity.components::<(&Rollback, Option<&S::Target>)>();

            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(_), Some(snapshot)) => {
                    if <S::Target as Component>::Mutability::MUTABLE {
                        unsafe {
                            // Error: get_mut_assume_mutable doesn't exist for EntityRef
                            let mut component = entity
                                .get_mut_assume_mutable::<S::Target>()
                                .expect("Failed to get mutable component");
                            S::update(component.as_mut(), snapshot);
                        }
                    } else {
                        commands.entity(entity.id()).insert(S::load(snapshot));
                    }
                }
                (Some(_), None) => {
                    commands.entity(entity.id()).remove::<S::Target>();
                }
                (None, Some(snapshot)) => {
                    commands.entity(entity.id()).insert(S::load(snapshot));
                }
                (None, None) => {}
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            snapshot.iter().count(),
            disqualified::ShortName::of::<S::Target>()
        );
    }
}

impl<S> Plugin for ComponentSnapshotPlugin<S>
where
    S: Send + Sync + 'static + Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<S::Target, S::Stored>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<S::Target, S::Stored>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            );
        app.add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

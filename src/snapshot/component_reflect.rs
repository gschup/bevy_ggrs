use crate::{
    GgrsComponentSnapshot, GgrsComponentSnapshots, LoadWorld, LoadWorldSet, Rollback,
    RollbackFrameCount, SaveWorld, SaveWorldSet,
};
use bevy::prelude::*;
use std::marker::PhantomData;

/// A [`Plugin`] which manages snapshots for a [`Component`] `C` using [`Reflect`] and [`FromWorld`].
///
/// NOTE: [`FromWorld`] is implemented for all types implementing [`Default`].
pub struct GgrsComponentSnapshotReflectPlugin<C>
where
    C: Component + Reflect + FromWorld,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for GgrsComponentSnapshotReflectPlugin<C>
where
    C: Component + Reflect + FromWorld,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<C> GgrsComponentSnapshotReflectPlugin<C>
where
    C: Component + Reflect + FromWorld,
{
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<C, Box<dyn Reflect>>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&Rollback, &C)>,
    ) {
        let components = query
            .iter()
            .map(|(&rollback, component)| (rollback, component.as_reflect().clone_value()));
        let snapshot = GgrsComponentSnapshot::new(components);
        snapshots.push(frame.0, snapshot);
    }

    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<C, Box<dyn Reflect>>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<(Entity, &Rollback, Option<&mut C>)>,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        for (entity, rollback, component) in query.iter_mut() {
            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(mut component), Some(snapshot)) => {
                    component.apply(snapshot.as_ref());
                }
                (Some(_), None) => {
                    commands.entity(entity).remove::<C>();
                }
                (None, Some(snapshot)) => {
                    let snapshot = snapshot.clone_value();

                    commands.add(move |world: &mut World| {
                        let mut component = C::from_world(world);
                        component.apply(snapshot.as_ref());
                        world.entity_mut(entity).insert(component);
                    })
                }
                (None, None) => {}
            }
        }
    }
}

impl<C> Plugin for GgrsComponentSnapshotReflectPlugin<C>
where
    C: Component + Reflect + FromWorld,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<C, Box<dyn Reflect>>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<C, Box<dyn Reflect>>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSet::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSet::Data));
    }
}

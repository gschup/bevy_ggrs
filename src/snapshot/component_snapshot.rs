//! Snapshot and restore of [`Component`] data on rollback entities.
//!
//! [`ComponentSnapshotPlugin`] saves all instances of a component type each frame and
//! restores them during rollback using a configurable [`Strategy`].
//! [`ImmutableComponentSnapshotPlugin`] provides the same behaviour for components
//! marked `#[component(immutable)]`, which must be re-inserted rather than mutated in place.

use crate::{
    ClearSnapshots, GgrsComponentSnapshot, GgrsComponentSnapshots, HeldComponentSnapshot,
    LoadWorld, LoadWorldSystems, RollbackFrameCount, RollbackId, SaveWorld, SaveWorldSystems,
    Strategy, stage_insert,
};
use bevy::{
    ecs::{
        component::{Immutable, Mutable},
        system::SystemChangeTick,
    },
    prelude::*,
};
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
    /// System that snapshots all instances of the component on rollback entities for this frame.
    ///
    /// When none has changed since the snapshot the world holds
    /// ([`HeldComponentSnapshot`]), that snapshot is pushed again, shared,
    /// rather than every component copied.
    pub fn save(
        mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
        mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(&RollbackId, Ref<S::Target>)>,
        ticks: SystemChangeTick,
    ) {
        let last_changed = query.iter().map(|(_, component)| component.last_changed());
        if let Some(snapshot) = held.unchanged(last_changed, ticks.this_run()) {
            let snapshot = snapshot.clone();
            trace!(
                "Snapshot {} {} component(s), unchanged",
                snapshot.len(),
                disqualified::ShortName::of::<S::Target>()
            );
            snapshots.push(frame.0, snapshot);
            return;
        }

        let components = query
            .iter()
            .map(|(&rollback, component)| (rollback, S::store(&component)));

        let snapshot = GgrsComponentSnapshot::new(components);

        trace!(
            "Snapshot {} {} component(s)",
            snapshot.len(),
            disqualified::ShortName::of::<S::Target>()
        );

        held.set(snapshot.clone(), ticks.this_run());
        snapshots.push(frame.0, snapshot);
    }
}

impl<S> ComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component<Mutability = Mutable>,
    S::Stored: Send + Sync + 'static,
{
    /// System that restores the component to its snapshotted state for the target frame,
    /// removing it or staging it for insertion as required. A component the entity lacks is
    /// inserted by [`insert_staged_components`](crate::insert_staged_components), together
    /// with the rest of the entity's staged components.
    ///
    /// A frame whose snapshot is the one the world still holds
    /// ([`HeldComponentSnapshot`]) is skipped: nothing is written, so nothing
    /// is marked changed, and the saves of the resimulated frames share it.
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
        mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        mut query: Query<(Entity, &RollbackId, Option<Mut<S::Target>>)>,
        ticks: SystemChangeTick,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        let last_changed = query
            .iter()
            .filter_map(|(_, _, component)| component.map(|c| c.last_changed()));
        if held
            .unchanged(last_changed, ticks.this_run())
            .is_some_and(|world| world.same(snapshot))
        {
            return;
        }
        held.set(snapshot.clone(), ticks.this_run());

        for (entity, rollback, component) in query.iter_mut() {
            let snapshot = snapshot.get(rollback);

            match (component, snapshot) {
                (Some(mut component), Some(snapshot)) => S::update(component.as_mut(), snapshot),
                (Some(_), None) => {
                    commands.entity(entity).remove::<S::Target>();
                }
                (None, Some(snapshot)) => {
                    commands.queue(stage_insert(entity, *rollback, S::load(snapshot)));
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
    S::Target: Component<Mutability = Mutable>,
    S::Stored: Send + Sync + 'static,
{
    /// Registers snapshot storage and the save/load systems for this component type.
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<S::Target, S::Stored>>()
            .init_resource::<HeldComponentSnapshot<S::Target, S::Stored>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<S::Target, S::Stored>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            );
        app.add_systems(
            LoadWorld,
            Self::load.in_set(LoadWorldSystems::ComponentData),
        );
        app.add_observer(
            |_trigger: On<ClearSnapshots>,
             mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
             mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>| {
                snapshots.clear();
                held.clear();
            },
        );
    }
}

/// A [`Plugin`] which manages snapshots for a [`Component`] using a provided [`Strategy`] that works with immutable components.
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ImmutableComponentSnapshotPlugin, CloneStrategy};
/// #
/// # fn start() {
/// # let mut app = App::new();
/// #[derive(Component, Clone)]
/// #[component(immutable)]
/// struct MyComponent(String);
///
/// app.add_plugins(ImmutableComponentSnapshotPlugin::<CloneStrategy<MyComponent>>::default());
/// # }
/// ```
pub struct ImmutableComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component,
    S::Stored: Send + Sync + 'static,
{
    _phantom: PhantomData<S>,
}

impl<S> Default for ImmutableComponentSnapshotPlugin<S>
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

impl<S> Plugin for ImmutableComponentSnapshotPlugin<S>
where
    S: Send + Sync + 'static + Strategy,
    S::Target: Component<Mutability = Immutable>,
    S::Stored: Send + Sync + 'static,
{
    /// Registers snapshot storage and the save/load systems for this immutable component type.
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<S::Target, S::Stored>>()
            .init_resource::<HeldComponentSnapshot<S::Target, S::Stored>>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<S::Target, S::Stored>::discard_old_snapshots,
                    ComponentSnapshotPlugin::<S>::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(
                LoadWorld,
                Self::load.in_set(LoadWorldSystems::ComponentData),
            );
        app.add_observer(
            |_trigger: On<ClearSnapshots>,
             mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
             mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>| {
                snapshots.clear();
                held.clear();
            },
        );
    }
}

impl<S> ImmutableComponentSnapshotPlugin<S>
where
    S: Strategy,
    S::Target: Component<Mutability = Immutable>,
    S::Stored: Send + Sync + 'static,
{
    /// System that restores this immutable component to its snapshotted state for the target frame,
    /// re-inserting it (triggering hooks) or removing it as required. A component the entity
    /// lacks is inserted by [`insert_staged_components`](crate::insert_staged_components),
    /// together with the rest of the entity's staged components.
    ///
    /// Skipped, as the mutable one is, when the frame's snapshot is the one the
    /// world still holds: no component is re-inserted and no hook fires.
    pub fn load(
        mut commands: Commands,
        mut snapshots: ResMut<GgrsComponentSnapshots<S::Target, S::Stored>>,
        mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>,
        frame: Res<RollbackFrameCount>,
        query: Query<(Entity, &RollbackId, Option<Ref<S::Target>>)>,
        ticks: SystemChangeTick,
    ) {
        let snapshot = snapshots.rollback(frame.0).get();

        let last_changed = query
            .iter()
            .filter_map(|(_, _, component)| component.map(|c| c.last_changed()));
        if held
            .unchanged(last_changed, ticks.this_run())
            .is_some_and(|world| world.same(snapshot))
        {
            return;
        }
        // Re-inserting is a command, applied after this system: the inserts
        // are newer than this tick, so the next save copies. Correct, and no
        // worse than before.
        held.set(snapshot.clone(), ticks.this_run());

        for (entity, rollback, component) in query.iter() {
            let has_component = component.is_some();
            let snapshot = snapshot.get(rollback);

            match (has_component, snapshot) {
                (true, None) => {
                    commands.entity(entity).remove::<S::Target>();
                }
                (true, Some(snapshot)) => {
                    commands.entity(entity).insert(S::load(snapshot));
                }
                (false, Some(snapshot)) => {
                    commands.queue(stage_insert(entity, *rollback, S::load(snapshot)));
                }
                (false, None) => {}
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            snapshot.iter().count(),
            disqualified::ShortName::of::<S::Target>()
        );
    }
}

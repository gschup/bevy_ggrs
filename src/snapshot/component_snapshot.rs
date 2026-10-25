//! Snapshot and restore of [`Component`] data on rollback entities.
//!
//! [`ComponentSnapshotPlugin`] saves all instances of a component type each frame and
//! restores them during rollback using a configurable [`Strategy`].
//! [`ImmutableComponentSnapshotPlugin`] provides the same behaviour for components
//! marked `#[component(immutable)]`, which must be re-inserted rather than mutated in place.

use crate::{
    ClearSnapshots, GgrsComponentSnapshots, HeldComponentSnapshot, LoadWorld, LoadWorldSystems,
    Restore, RollbackId, SaveWorld, SaveWorldSystems, SharedSnapshots, Strategy, stage_insert,
};
use bevy::{
    ecs::{
        change_detection::CheckChangeTicks,
        component::{Immutable, Mutable},
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
    /// Only the chunks of entities holding a component changed, added or
    /// removed since the snapshot the world holds ([`HeldComponentSnapshot`])
    /// are copied; the rest are shared with it.
    pub fn save(
        mut store: SharedSnapshots<S::Target, S::Stored>,
        query: Query<(&RollbackId, Ref<S::Target>)>,
        changed: Query<Ref<S::Target>, With<RollbackId>>,
    ) {
        let len = store.save(
            changed.iter().map(|component| component.last_changed()),
            query
                .iter()
                .map(|(&rollback, component)| (rollback, component.last_changed())),
            query
                .iter()
                .map(|(&rollback, component)| (rollback, component)),
            |component| S::store(&component),
        );

        trace!(
            "Snapshot {} {} component(s)",
            len,
            disqualified::ShortName::of::<S::Target>()
        );
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
    /// A chunk of the frame's snapshot that is the one the world still holds
    /// ([`HeldComponentSnapshot`]) is skipped: nothing there is written, so
    /// nothing is marked changed, and the saves of the resimulated frames share
    /// it.
    pub fn load(
        mut commands: Commands,
        mut store: SharedSnapshots<S::Target, S::Stored>,
        mut with: Query<(Entity, &RollbackId, Mut<S::Target>)>,
        without: Query<(Entity, &RollbackId), Without<S::Target>>,
    ) {
        let Some(plan) = store.load(
            with.iter()
                .map(|(_, _, component)| component.last_changed()),
            with.iter()
                .map(|(_, &rollback, component)| (rollback, component.last_changed())),
        ) else {
            return;
        };

        // The entities holding the type, then, only when the snapshot holds
        // some they do not, the rest: a type on a few entities walks those
        // few, not every rollback entity.
        let mut matched = 0;
        with.iter_mut()
            .for_each(
                |(entity, rollback, mut component)| match plan.restore(rollback) {
                    Restore::Shared => {}
                    // Read without marking it changed; written only when it differs.
                    Restore::Stored(snapshot) => {
                        matched += 1;
                        if !S::equals(&component, snapshot) {
                            S::update(component.as_mut(), snapshot);
                        }
                    }
                    Restore::Absent => {
                        commands.entity(entity).remove::<S::Target>();
                    }
                },
            );
        if plan.holds_more(matched) {
            for (entity, rollback) in &without {
                if let Some(snapshot) = plan.missing(rollback) {
                    commands.queue(stage_insert(entity, *rollback, S::load(snapshot)));
                }
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            plan.to_write(),
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
        app.add_observer(
            |check: On<CheckChangeTicks>,
             mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>| {
                held.check_change_ticks(*check);
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
        app.add_observer(
            |check: On<CheckChangeTicks>,
             mut held: ResMut<HeldComponentSnapshot<S::Target, S::Stored>>| {
                held.check_change_ticks(*check);
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
    /// Chunks are skipped as the mutable one skips them: where the frame's
    /// snapshot is the one the world still holds, no component is re-inserted
    /// and no hook fires.
    pub fn load(
        mut commands: Commands,
        mut store: SharedSnapshots<S::Target, S::Stored>,
        with: Query<(Entity, &RollbackId, Ref<S::Target>)>,
        without: Query<(Entity, &RollbackId), Without<S::Target>>,
    ) {
        // Re-inserting is a command, applied after this system: the inserts
        // are newer than this tick, so the next save copies their chunks.
        // Correct, and no worse than before.
        let Some(plan) = store.load(
            with.iter()
                .map(|(_, _, component)| component.last_changed()),
            with.iter()
                .map(|(_, &rollback, component)| (rollback, component.last_changed())),
        ) else {
            return;
        };

        // As the mutable load: the holders, then the rest only when needed.
        let mut matched = 0;
        for (entity, rollback, component) in &with {
            match plan.restore(rollback) {
                Restore::Shared => {}
                Restore::Stored(snapshot) => {
                    matched += 1;
                    if !S::equals(&component, snapshot) {
                        commands.entity(entity).insert(S::load(snapshot));
                    }
                }
                Restore::Absent => {
                    commands.entity(entity).remove::<S::Target>();
                }
            }
        }
        if plan.holds_more(matched) {
            for (entity, rollback) in &without {
                if let Some(snapshot) = plan.missing(rollback) {
                    commands.queue(stage_insert(entity, *rollback, S::load(snapshot)));
                }
            }
        }

        trace!(
            "Rolled back {} {} component(s)",
            plan.to_write(),
            disqualified::ShortName::of::<S::Target>()
        );
    }
}

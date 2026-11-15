//! Snapshot and restore of [`Component`] data on rollback entities.
//!
//! [`ComponentSnapshotPlugin`] saves all instances of a component type each frame and
//! restores them during rollback using a configurable [`Strategy`].
//! [`ImmutableComponentSnapshotPlugin`] provides the same behaviour for components
//! marked `#[component(immutable)]`, which must be re-inserted rather than mutated in place.

use crate::{
    ChunksToLoad, ClearSnapshots, GgrsComponentSnapshot, GgrsComponentSnapshots,
    HeldComponentSnapshot, LoadWorld, LoadWorldSystems, Restore, RollbackEntities, RollbackId,
    SaveWorld, SaveWorldSystems, SharedSnapshots, Strategy, TableRows, TablesLoad, stage_insert,
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
    /// are copied; the rest are shared with it. Tables where nothing changed
    /// are not read ([`TableRows`]), unless the type is stored in sparse sets.
    pub fn save(
        mut store: SharedSnapshots<S::Target, S::Stored>,
        query: Query<(Ref<RollbackId>, Ref<S::Target>)>,
    ) {
        let len = match query.contiguous_iter() {
            Ok(tables) => {
                let tables: Vec<_> = tables
                    .map(|(ids, values)| TableRows::new(ids, values))
                    .collect();
                store.save_tables(&tables, S::store, S::copy)
            }
            Err(_) => store.save(
                query.iter().map(|(_, component)| component.last_changed()),
                query
                    .iter()
                    .map(|(rollback, component)| (*rollback, component.last_changed())),
                query
                    .iter()
                    .map(|(rollback, component)| (*rollback, component)),
                |component| S::store(&component),
            ),
        };

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
    ///
    /// Which chunks those are is found table by table
    /// ([`HeldComponentSnapshot::plan_tables`]), and only their entities are
    /// visited, found through [`RollbackEntities`]: a load costs what differs,
    /// not every entity holding the type. Where the tables cannot vouch for
    /// the world, or an entity is not where the map says, it walks them all.
    pub fn load(
        mut commands: Commands,
        mut store: SharedSnapshots<S::Target, S::Stored>,
        mut queries: ParamSet<(
            Query<(Ref<RollbackId>, Ref<S::Target>)>,
            Query<(Entity, &RollbackId, Mut<S::Target>)>,
        )>,
        without: Query<(Entity, &RollbackId), Without<S::Target>>,
        entities: Res<RollbackEntities>,
    ) {
        let plan = {
            let tables = queries.p0();
            match tables.contiguous_iter() {
                Ok(tables) => {
                    let tables: Vec<_> = tables
                        .map(|(ids, values)| TableRows::new(ids, values))
                        .collect();
                    store.plan_tables(&tables)
                }
                Err(_) => TablesLoad::Walk,
            }
        };
        let chunks = match plan {
            TablesLoad::Nothing => {
                store.loaded();
                return;
            }
            TablesLoad::Chunks(chunks) => chunks,
            TablesLoad::Walk => {
                Self::walk(&mut commands, &mut store, &mut queries.p1(), &without);
                return;
            }
        };
        let mut with = queries.p1();
        let Some(writes) = resolve(&chunks, store.frame_snapshot(), &with, &without, &entities)
        else {
            Self::walk(&mut commands, &mut store, &mut with, &without);
            return;
        };
        let snapshot = store.frame_snapshot();
        for write in &writes {
            match *write {
                Write::Update(entity, rollback) => {
                    let stored = snapshot.get(&rollback).expect("resolved from it");
                    let (_, _, mut component) = with.get_mut(entity).expect("resolved");
                    // Read without marking it changed; written only when it differs.
                    if !S::equals(&component, stored) {
                        S::update(component.as_mut(), stored);
                    }
                }
                Write::Remove(entity) => {
                    commands.entity(entity).remove::<S::Target>();
                }
                Write::Insert(entity, rollback) => {
                    let stored = snapshot.get(&rollback).expect("resolved from it");
                    commands.queue(stage_insert(entity, rollback, S::load(stored)));
                }
            }
        }
        store.loaded();

        trace!(
            "Rolled back {} {} component(s) in {} chunk(s)",
            writes.len(),
            disqualified::ShortName::of::<S::Target>(),
            chunks.chunks.len()
        );
    }

    /// [`load`](Self::load) by walking every entity holding the type.
    fn walk(
        commands: &mut Commands,
        store: &mut SharedSnapshots<S::Target, S::Stored>,
        with: &mut Query<(Entity, &RollbackId, Mut<S::Target>)>,
        without: &Query<(Entity, &RollbackId), Without<S::Target>>,
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
            for (entity, rollback) in without {
                if let Some(snapshot) = plan.missing(rollback) {
                    commands.queue(stage_insert(entity, *rollback, S::load(snapshot)));
                }
            }
        }

        trace!(
            "Rolled back {} {} component(s) by walking",
            plan.to_write(),
            disqualified::ShortName::of::<S::Target>()
        );
    }
}

/// One write of a load by tables, to an entity found and checked before
/// anything is written.
enum Write {
    /// The entity holds the type: give it the snapshot's value.
    Update(Entity, RollbackId),
    /// The snapshot holds none for it: remove it.
    Remove(Entity),
    /// The entity lacks the type: stage the snapshot's value.
    Insert(Entity, RollbackId),
}

/// The writes a load of `chunks` makes, each to an entity [`RollbackEntities`]
/// names and whose own [`RollbackId`] agrees, or `None` if one does not: the
/// load then walks. An id of the snapshot whose entity is not in the world,
/// or is marked despawned, is left alone, as the walk leaves it.
fn resolve<T: Component<Mutability = Mutable>, As>(
    chunks: &ChunksToLoad,
    snapshot: &GgrsComponentSnapshot<T, As>,
    with: &Query<(Entity, &RollbackId, Mut<T>)>,
    without: &Query<(Entity, &RollbackId), Without<T>>,
    entities: &RollbackEntities,
) -> Option<Vec<Write>> {
    let holding = |rollback: RollbackId| {
        let entity = entities.get(rollback)?;
        let (_, &found, _) = with.get(entity).ok()?;
        (found == rollback).then_some(entity)
    };
    let mut writes = Vec::new();
    for &index in &chunks.chunks {
        let world = chunks.world_in(index);
        let entries = snapshot.chunk_entries(index);
        for &rollback in world {
            let entity = holding(rollback)?;
            if entries
                .binary_search_by_key(&rollback, |(id, _)| *id)
                .is_err()
            {
                writes.push(Write::Remove(entity));
            }
        }
        for &(rollback, _) in entries {
            if world.binary_search(&rollback).is_ok() {
                writes.push(Write::Update(holding(rollback)?, rollback));
                continue;
            }
            let Some(entity) = entities.get(rollback) else {
                continue;
            };
            if let Ok((_, &found)) = without.get(entity) {
                if found != rollback {
                    return None;
                }
                writes.push(Write::Insert(entity, rollback));
            } else if with.get(entity).is_ok() {
                // It holds the type, but the plan did not know: walk.
                return None;
            }
        }
    }
    Some(writes)
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

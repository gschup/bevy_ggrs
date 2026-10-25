//! Snapshot and restore of the rollback entity set.
//!
//! [`EntitySnapshotPlugin`] saves a mapping of [`RollbackId`] → [`Entity`] each frame.
//! On rollback, it reconciles the live entity set against the snapshot — spawning
//! missing entities, despawning extras, and recording any ID changes in a
//! [`RollbackEntityMap`] so that subsequent plugins can fix up stale entity references.

use crate::{
    ClearSnapshots, GgrsComponentSnapshots, HeldComponentSnapshot, LoadWorld, LoadWorldSystems,
    Restore, Rollback, RollbackEntityMap, RollbackId, SaveWorld, SaveWorldSystems, SharedSnapshots,
};
use bevy::{
    ecs::{change_detection::CheckChangeTicks, entity::EntityHashMap},
    platform::collections::HashSet,
    prelude::*,
};

/// A [`Plugin`] which manages the rollback for [`Entities`](`Entity`). This will ensure
/// all [`Entities`](`Entity`) match the state of the desired frame, or can be mapped using a
/// [`RollbackEntityMap`], which this [`Plugin`] will also manage.
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, EntitySnapshotPlugin};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// // This will ensure entities are updated on rollback to match the state of the target snapshot
/// app.add_plugins(EntitySnapshotPlugin);
/// # }
/// ```
pub struct EntitySnapshotPlugin;

impl EntitySnapshotPlugin {
    /// System that records the current [`RollbackId`] → [`Entity`] mapping for this frame.
    ///
    /// An id's entity changes only with its [`RollbackId`], which is
    /// immutable: only the chunks where one was inserted, or an entity went,
    /// are copied ([`HeldComponentSnapshot`]).
    pub fn save(
        mut store: SharedSnapshots<Entity, Entity>,
        query: Query<(Ref<RollbackId>, Entity)>,
    ) {
        let len = store.save(
            query.iter().map(|(rollback, _)| rollback.last_changed()),
            query
                .iter()
                .map(|(rollback, _)| (*rollback, rollback.last_changed())),
            query.iter().map(|(rollback, entity)| (*rollback, entity)),
            |entity| entity,
        );

        trace!("Snapshot {} entity(s)", len);
    }

    /// System that reconciles live entities against the snapshot for the target frame,
    /// spawning or despawning as needed and populating [`RollbackEntityMap`] with any ID changes.
    ///
    /// The live entities first, each looked up in the snapshot; then, only when the
    /// snapshot holds some the world does not, the snapshot, for those to respawn. A load
    /// that respawned nothing and found every entity under its own ID leaves the map
    /// empty. A chunk of entities that is the one the world holds is skipped.
    pub fn load(
        mut commands: Commands,
        mut store: SharedSnapshots<Entity, Entity>,
        mut map: ResMut<RollbackEntityMap>,
        query: Query<(Ref<RollbackId>, Entity)>,
    ) {
        let mut entity_map = EntityHashMap::<Entity>::default();

        let frame = store.frame();
        let Some(plan) = store.load(
            query.iter().map(|(rollback, _)| rollback.last_changed()),
            query
                .iter()
                .map(|(rollback, _)| (*rollback, rollback.last_changed())),
        ) else {
            *map = entity_map.into();
            return;
        };

        let snapshot = plan.snapshot();
        let mut matched = 0;
        for (rollback, current_entity) in &query {
            match plan.restore(&rollback) {
                Restore::Shared => {}
                Restore::Stored(&old_entity) => {
                    matched += 1;
                    if old_entity != current_entity {
                        entity_map.insert(old_entity, current_entity);
                    }
                }
                Restore::Absent => {
                    // A child the frame lacks along with its parent is gone
                    // already when the parent's despawn came first.
                    commands.entity(current_entity).try_despawn();
                }
            }
        }

        if plan.holds_more(matched) {
            let alive: HashSet<RollbackId> = query.iter().map(|(rollback, _)| *rollback).collect();
            for (&rollback, &old_entity) in snapshot.iter() {
                if alive.contains(&rollback) || plan.missing(&rollback).is_none() {
                    continue;
                }
                // A respawn is bare: it gets the components registered for rollback and
                // nothing else the entity had. A rollback entity despawned outright
                // while a snapshot still holds it ends here; `despawn_rollback` marks it
                // instead, and the mark lifts whole.
                warn!(
                    "respawning {rollback:?} bare: it was despawned while frame {} still \
                     held it; despawn_rollback keeps every component",
                    frame
                );
                let current_entity = commands.spawn((rollback, Rollback)).id();
                entity_map.insert(old_entity, current_entity);
            }
        }

        trace!("Rolled back {} entity(s)", snapshot.len());

        *map = entity_map.into();
    }
}

impl Plugin for EntitySnapshotPlugin {
    /// Registers entity snapshot storage, [`RollbackEntityMap`], and the save/load systems.
    fn build(&self, app: &mut App) {
        app.init_resource::<GgrsComponentSnapshots<Entity>>()
            .init_resource::<HeldComponentSnapshot<Entity>>()
            .init_resource::<RollbackEntityMap>()
            .add_systems(
                SaveWorld,
                (
                    GgrsComponentSnapshots::<Entity>::discard_old_snapshots,
                    Self::save,
                )
                    .chain()
                    .in_set(SaveWorldSystems::Snapshot),
            )
            .add_systems(LoadWorld, Self::load.in_set(LoadWorldSystems::Entity));
        app.add_observer(
            |_trigger: On<ClearSnapshots>,
             mut snapshots: ResMut<GgrsComponentSnapshots<Entity>>,
             mut held: ResMut<HeldComponentSnapshot<Entity>>| {
                snapshots.clear();
                held.clear();
            },
        );
        app.add_observer(
            |check: On<CheckChangeTicks>, mut held: ResMut<HeldComponentSnapshot<Entity>>| {
                held.check_change_ticks(*check);
            },
        );
    }
}

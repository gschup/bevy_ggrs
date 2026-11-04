//! Rollback marker component and stable entity identity.
//!
//! Add [`Rollback`] to any entity whose state should be saved and restored during rollback.
//! An [`on_add`](`bevy::ecs::lifecycle`) hook automatically assigns a stable
//! [`RollbackId`] and registers the entity in [`RollbackOrdered`], which provides a
//! deterministic iteration order across peers.

use std::sync::Arc;

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
    reflect::CreateTypeData,
};

/// Marker component that flags an entity for inclusion in the rollback save/load schedule.
///
/// Simply include this in your spawn bundle:
/// ```rust,ignore
/// commands.spawn((MyComponent, Rollback));
/// ```
///
/// An `on_add` hook will automatically create a [`RollbackId`] for the entity and
/// register it for stable ordering.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(on_add = on_rollback_added)]
pub struct Rollback;

/// A stable identifier for rollback entities, used as a key in snapshot storage.
/// Automatically inserted when [`Rollback`] is added to an entity.
///
/// `clone_behavior = Ignore` prevents `EntityCloner` from copying this value
/// onto a clone. `RollbackId` is an identity, not a value to share. A clone
/// instead receives a fresh `RollbackId` from `Rollback`'s `on_add` hook
/// firing on the new entity.
///
/// Its entity's despawn takes it out of [`RollbackOrdered`] (`on_remove`): an id
/// nothing in the world carries is never asked for its order, and a snapshot that holds it
/// holds its own [`RollbackOrdered`] with it.
#[derive(Component, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
#[component(
    immutable,
    summary_tick,
    clone_behavior = Ignore,
    on_remove = on_rollback_id_removed
)]
pub struct RollbackId(Entity);

impl RollbackId {
    /// Creates a new [`RollbackId`] from an [`Entity`].
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
    }

    /// The index of the entity the id was made from.
    #[inline(always)]
    pub(crate) fn index(&self) -> u32 {
        self.0.index_u32()
    }
}

fn on_rollback_added(mut world: DeferredWorld, ctx: HookContext) {
    let entity = ctx.entity;

    // Respawn path: RollbackId already present from bundle (e.g. during rollback restore).
    // RollbackOrdered is independently restored by its own snapshot, so no push needed.
    if world.get::<RollbackId>(entity).is_some() {
        return;
    }

    // Normal path: create new RollbackId and register for ordering
    let rollback_id = RollbackId::new(entity);
    world.commands().entity(entity).insert(rollback_id);
    let mut ordered = world.resource_mut::<RollbackOrdered>();
    ordered.push(rollback_id);
}

/// A [`RollbackId`] leaves the world only with its entity, so its order goes with it.
///
/// A load that despawns an entity its frame lacked forgets an id that frame's
/// [`RollbackOrdered`], restored after, never had.
fn on_rollback_id_removed(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&rollback) = world.get::<RollbackId>(ctx.entity) else {
        return;
    };
    if let Some(mut ordered) = world.get_resource_mut::<RollbackOrdered>() {
        ordered.forget(rollback);
    }
}

/// How many frames apart [`RollbackOrdered`] drops ids no entity carries: a load of a frame
/// saved before a despawn can bring a forgotten id back (see [`prune_rollback_ordered`]), and
/// the table then holds at most what that many frames' loads restored. A game that spawns and
/// despawns many short-lived rollback entities may want it shorter.
///
/// Defaults to `Some(128)`. `None` never prunes.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollbackOrderedPrune(pub Option<u32>);

impl Default for RollbackOrderedPrune {
    fn default() -> Self {
        Self(Some(128))
    }
}

/// Every [`RollbackOrderedPrune`] frames, after the save, drop from
/// [`RollbackOrdered`] every id no entity in the world carries.
///
/// A despawn forgets its id, but a frame saved before the despawn holds a table with the id
/// in it: a despawn mark lets go of its entity once no frame from before the mark is held,
/// while frames from after it, which still list the id, may be. A load of one of those brings
/// the id back, and no entity is left to take it out. Which ids the table holds is not
/// simulated state (numbers never move, and the entity checksum hashes
/// [`issued`](RollbackOrdered::issued)), so this may run on any frame, on any peer.
pub(crate) fn prune_rollback_ordered(
    frame: Res<crate::RollbackFrameCount>,
    prune: Res<RollbackOrderedPrune>,
    mut ordered: ResMut<RollbackOrdered>,
    carried: Query<&RollbackId, bevy::ecs::query::Allow<crate::RollbackDespawned>>,
) {
    let Some(every) = prune.0 else {
        return;
    };
    if frame.0.rem_euclid(every.max(1) as i32) != 0 {
        return;
    }
    let live: bevy::platform::collections::HashSet<RollbackId> = carried.iter().copied().collect();
    // Copied only when something goes.
    if ordered.order.keys().all(|id| live.contains(id)) {
        return;
    }
    Arc::make_mut(&mut ordered.order).retain(|id, _| live.contains(id));
}

/// A [`Resource`] which provides methods for stable ordering of [`RollbackId`] components.
///
/// Every id gets the next number when it is first registered and keeps it while its entity is
/// in the world. A despawn forgets it, so the table holds the rollback entities there are, not
/// every one there ever was: a session that keeps spawning and despawning (a field whose far
/// rocks dehydrate) keeps it, and each snapshot of it, the size of the world. The numbers do
/// not move when one is forgotten, so they stay what every peer agrees on.
///
/// Snapshotted every frame and changed only by a spawn or a despawn, so a clone shares the
/// data and a [`push`](Self::push) or a forget copies it.
#[derive(Resource, Default, Clone)]
pub struct RollbackOrdered {
    order: Arc<HashMap<RollbackId, u64>>,
    /// The number the next id registered gets.
    next: u64,
}

impl RollbackOrdered {
    /// Register a new [`RollbackId`] for explicit ordering.
    /// Idempotent: if the id is already registered, this is a no-op.
    pub(crate) fn push(&mut self, rollback: RollbackId) -> &mut Self {
        if self.order.contains_key(&rollback) {
            return self;
        }
        Arc::make_mut(&mut self.order).insert(rollback, self.next);
        self.next += 1;

        self
    }

    /// Forget `rollback`: its entity left the world. Its number is never given again.
    fn forget(&mut self, rollback: RollbackId) {
        if self.order.contains_key(&rollback) {
            Arc::make_mut(&mut self.order).remove(&rollback);
        }
    }

    /// Iterate over every registered [`RollbackId`] in order: those whose entity is in the
    /// world, marked despawned or not.
    pub fn iter_sorted(&self) -> impl Iterator<Item = RollbackId> + '_ {
        let mut sorted: Vec<(u64, RollbackId)> =
            self.order.iter().map(|(&id, &order)| (order, id)).collect();
        sorted.sort_unstable();
        sorted.into_iter().map(|(_, id)| id)
    }

    /// Returns a unique and order stable index for the provided [`RollbackId`].
    pub fn order(&self, rollback: RollbackId) -> u64 {
        self.order
            .get(&rollback)
            .copied()
            .expect("RollbackId was not registered in RollbackOrdered!")
    }

    /// How many ids have been registered, forgotten ones included: the number the next one
    /// gets. Rolled back with the table, so every peer agrees on it, where they need not agree
    /// on [`len`](Self::len) while a despawn mark is held on one and gone on the other.
    pub fn issued(&self) -> u64 {
        self.next
    }

    /// Get the number of registered rollback entities: those whose entity is in the world,
    /// marked despawned or not.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Returns `true` if there are no registered rollback entities, false otherwise.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::entity::EntityCloner, prelude::*};

    use super::{Rollback, RollbackId, RollbackOrdered};
    use crate::snapshot::SnapshotPlugin;

    fn id(n: u32) -> RollbackId {
        RollbackId::new(Entity::from_raw_u32(n).expect("valid test entity index"))
    }

    fn ordered_with(ids: &[u32]) -> RollbackOrdered {
        let mut ro = RollbackOrdered::default();
        for &n in ids {
            ro.push(id(n));
        }
        ro
    }

    /// Each ID receives a unique, zero-based, insertion-order index.
    #[test]
    fn order_returns_insertion_index() {
        let ro = ordered_with(&[10, 20, 30]);
        assert_eq!(ro.order(id(10)), 0);
        assert_eq!(ro.order(id(20)), 1);
        assert_eq!(ro.order(id(30)), 2);
    }

    /// `iter_sorted` yields IDs in insertion order.
    #[test]
    fn iter_sorted_yields_insertion_order() {
        let ro = ordered_with(&[5, 3, 7, 1]);
        let got: Vec<RollbackId> = ro.iter_sorted().collect();
        assert_eq!(got, vec![id(5), id(3), id(7), id(1)]);
    }

    /// Indices are stable — pushing more IDs does not change existing indices.
    #[test]
    fn order_is_stable_after_more_pushes() {
        let mut ro = ordered_with(&[0, 1]);
        let order_0_before = ro.order(id(0));
        let order_1_before = ro.order(id(1));
        ro.push(id(2));
        ro.push(id(3));
        assert_eq!(ro.order(id(0)), order_0_before);
        assert_eq!(ro.order(id(1)), order_1_before);
    }

    /// Querying the order of an unregistered ID panics with the expected message.
    #[test]
    #[should_panic(expected = "RollbackId was not registered in RollbackOrdered!")]
    fn order_unregistered_panics() {
        let ro = ordered_with(&[0]);
        ro.order(id(99));
    }

    /// Regression: `EntityCloner` must not copy `RollbackId` from the source.
    /// Before `clone_behavior = Ignore`, the clone inherited the source's id
    /// and `EntitySnapshotPlugin` couldn't tell them apart during save/load.
    #[test]
    fn entity_cloner_assigns_fresh_rollback_id() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);

        let source = app.world_mut().spawn(Rollback).id();
        app.update();
        let source_rid = *app
            .world()
            .get::<RollbackId>(source)
            .expect("source has RollbackId");

        let clone = EntityCloner::default().spawn_clone(app.world_mut(), source);
        app.update();
        let clone_rid = *app
            .world()
            .get::<RollbackId>(clone)
            .expect("clone has RollbackId");

        assert_ne!(
            source_rid, clone_rid,
            "clone must have a unique RollbackId, got the source's id"
        );
    }

    /// A forgotten id takes no number with it: the next id gets the next number, and the
    /// others keep theirs.
    #[test]
    fn forget_keeps_every_other_number() {
        let mut ro = ordered_with(&[10, 20, 30]);
        ro.forget(id(20));
        assert_eq!(ro.len(), 2);
        assert_eq!(ro.issued(), 3);
        ro.push(id(40));
        assert_eq!(ro.order(id(10)), 0);
        assert_eq!(ro.order(id(30)), 2);
        assert_eq!(ro.order(id(40)), 3);
        let got: Vec<RollbackId> = ro.iter_sorted().collect();
        assert_eq!(got, vec![id(10), id(30), id(40)]);
    }

    /// A despawn forgets its entity's id; a snapshot taken before keeps it.
    #[test]
    fn a_despawn_forgets_the_id() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        let kept = app.world_mut().spawn(Rollback).id();
        let gone = app.world_mut().spawn(Rollback).id();
        app.update();
        let gone_id = *app.world().get::<RollbackId>(gone).unwrap();
        let before = app.world().resource::<RollbackOrdered>().clone();
        app.world_mut().entity_mut(gone).despawn();
        let ordered = app.world().resource::<RollbackOrdered>();
        assert_eq!(ordered.len(), 1);
        let kept_id = *app.world().get::<RollbackId>(kept).unwrap();
        assert_eq!(ordered.order(kept_id), 0);
        assert_eq!(
            before.order(gone_id),
            1,
            "the clone a snapshot holds keeps it"
        );
    }

    /// Cloning `RollbackOrdered` produces an independent copy with identical ordering.
    #[test]
    fn clone_is_independent() {
        let ro = ordered_with(&[1, 2, 3]);
        let mut clone = ro.clone();
        clone.push(id(4));
        // The original must be unaffected
        assert_eq!(ro.len(), 3);
        assert_eq!(clone.len(), 4);
        assert_eq!(ro.order(id(1)), clone.order(id(1)));
    }
}

/// Trait for types that know how to register themselves for rollback.
/// Typically generated by `#[derive(Rollback)]`.
pub trait RollbackRegistration {
    /// Register this type's rollback configuration with the app.
    fn register_rollback(app: &mut App);
}

/// Reflect type data for rollback registration. Enables auto-discovery of rollback
/// types through Bevy's type registry when used with `#[reflect(Rollback)]`.
#[derive(Clone)]
pub struct ReflectRollback {
    pub(crate) register_fn: fn(&mut App),
}

impl ReflectRollback {
    /// Register the rollback configuration for this type with the app.
    pub fn register(&self, app: &mut App) {
        (self.register_fn)(app);
    }
}

impl<T: RollbackRegistration + 'static> CreateTypeData<T> for ReflectRollback {
    fn create_type_data(_input: ()) -> Self {
        ReflectRollback {
            register_fn: T::register_rollback,
        }
    }
}

/// Marker resource that disables auto-registration of rollback types from the type registry.
#[derive(Resource)]
pub(crate) struct SkipAutoRegistration;

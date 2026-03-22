//! Rollback marker component and stable entity identity.
//!
//! Add [`Rollback`] to any entity whose state should be saved and restored during rollback.
//! An [`on_add`](`bevy::ecs::lifecycle`) hook automatically assigns a stable
//! [`RollbackId`] and registers the entity in [`RollbackOrdered`], which provides a
//! deterministic iteration order across peers.

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
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
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Debug)]
#[component(immutable)]
pub struct RollbackId(Entity);

impl RollbackId {
    /// Creates a new [`RollbackId`] from an [`Entity`].
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
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

/// A [`Resource`] which provides methods for stable ordering of [`RollbackId`] components.
#[derive(Resource, Default, Clone)]
pub struct RollbackOrdered {
    order: HashMap<RollbackId, u64>,
    sorted: Vec<RollbackId>,
}

impl RollbackOrdered {
    /// Register a new [`RollbackId`] for explicit ordering.
    fn push(&mut self, rollback: RollbackId) -> &mut Self {
        self.sorted.push(rollback);
        self.order.insert(rollback, self.sorted.len() as u64 - 1);

        self
    }

    /// Iterate over all [`RollbackId`] markers ever registered, even if they have since been deleted.
    pub fn iter_sorted(&self) -> impl Iterator<Item = RollbackId> + '_ {
        self.sorted.iter().copied()
    }

    /// Returns a unique and order stable index for the provided [`RollbackId`].
    pub fn order(&self, rollback: RollbackId) -> u64 {
        self.order
            .get(&rollback)
            .copied()
            .expect("RollbackId was not registered in RollbackOrdered!")
    }

    /// Get the number of registered rollback entities.
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
    use bevy::prelude::Entity;

    use super::{RollbackId, RollbackOrdered};

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

    /// A freshly created `RollbackOrdered` is empty.
    #[test]
    fn default_is_empty() {
        let ro = RollbackOrdered::default();
        assert!(ro.is_empty());
        assert_eq!(ro.len(), 0);
    }

    /// Pushing IDs increments len and clears is_empty.
    #[test]
    fn push_increases_len() {
        let ro = ordered_with(&[0, 1, 2]);
        assert!(!ro.is_empty());
        assert_eq!(ro.len(), 3);
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

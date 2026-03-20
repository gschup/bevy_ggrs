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
/// An [`on_add`] hook will automatically create a [`RollbackId`] for the entity and
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

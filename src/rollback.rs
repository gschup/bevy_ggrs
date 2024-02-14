use bevy::utils::HashMap;
use bevy::{
    ecs::system::{EntityCommand, EntityCommands},
    prelude::*,
};

/// This component flags an entity as being included in the rollback save/load schedule with GGRS.
///
/// You must use the [`AddRollbackCommand`] when spawning an entity to add this component. Alternatively,
/// you can use the `add_rollback()` extension method provided by [`AddRollbackCommandExtension`].
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Rollback(Entity);

impl Rollback {
    /// Creates a new [`Rollback`] component from an [`Entity`].
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
    }
}

/// An [`EntityCommand`] which adds a [`Rollback`] component to an entity.
pub struct AddRollbackCommand;

impl EntityCommand for AddRollbackCommand {
    fn apply(self, id: Entity, world: &mut World) {
        let rollback = Rollback::new(id);

        world.entity_mut(id).insert(rollback);

        world
            .get_resource_or_insert_with::<RollbackOrdered>(default)
            .push(rollback);
    }
}

mod private {
    /// Private seal to ensure [`AddRollbackCommandExtension`](`super::AddRollbackCommandExtension`) cannot be implemented by crate consumers.
    pub trait AddRollbackCommandExtensionSeal {}
}

/// Extension trait for [`EntityCommands`] which adds the `add_rollback()` method.
pub trait AddRollbackCommandExtension: private::AddRollbackCommandExtensionSeal {
    /// Adds an automatically generated `Rollback` component to this `Entity`.
    fn add_rollback(&mut self) -> &mut Self;
}

impl<'w, 's, 'a> private::AddRollbackCommandExtensionSeal for EntityCommands<'w, 's, 'a> {}

impl<'w, 's, 'a> AddRollbackCommandExtension for EntityCommands<'w, 's, 'a> {
    fn add_rollback(&mut self) -> &mut Self {
        self.add(AddRollbackCommand);
        self
    }
}

/// A [`Resource`] which provides methods for stable ordering of [`Rollback`] flags.
#[derive(Resource, Default, Clone)]
pub struct RollbackOrdered {
    order: HashMap<Rollback, u64>,
    sorted: Vec<Rollback>,
}

impl RollbackOrdered {
    /// Register a new [`Rollback`] for explicit ordering.
    fn push(&mut self, rollback: Rollback) -> &mut Self {
        self.sorted.push(rollback);
        self.order.insert(rollback, self.sorted.len() as u64 - 1);

        self
    }

    /// Iterate over all [`Rollback`] markers ever registered, even if they have since been deleted.
    pub fn iter_sorted(&self) -> impl Iterator<Item = Rollback> + '_ {
        self.sorted.iter().copied()
    }

    /// Returns a unique and order stable index for the provided [`Rollback`].
    pub fn order(&self, rollback: Rollback) -> u64 {
        self.order
            .get(&rollback)
            .copied()
            .expect("Rollback requested was not created using AddRollbackCommand!")
    }

    /// Get the number of registered [`Rollback`] entities.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Returns `true` if there are no registered [`Rollback`] entities, false otherwise.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

use bevy::{prelude::{Component, Entity, World}, ecs::system::{EntityCommand, EntityCommands}};

/// This component flags an entity as being included in the rollback save/load schedule with GGRS.
/// 
/// You must use the `AddRollbackCommand` when spawning an entity to add this component. Alternatively,
/// you can use the `add_rollback()` extension method provided by `AddRollbackCommandExtension`.
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Rollback(Entity);

impl Rollback {
    /// Creates a new `Rollback` component from an `Entity`.
    pub(crate) fn new(entity: Entity) -> Self {
        Self(entity)
    }
}

/// An `EntityCommand` which adds a `Rollback` component to an entity.
pub struct AddRollbackCommand;

impl EntityCommand for AddRollbackCommand {
    fn write(self, id: Entity, world: &mut World) {
        world.entity_mut(id).insert(Rollback::new(id));
    }
}

mod private {
    /// Private seal to ensure `AddRollbackCommandExtension` cannot be implemented by crate consumers.
    pub trait AddRollbackCommandExtensionSeal {}
}

/// Extension trait for `EntityCommands` which adds the `add_rollback()` method.
pub trait AddRollbackCommandExtension: private::AddRollbackCommandExtensionSeal {
    /// Adds an automatically generated `Rollback` component to this `Entity`.
    fn add_rollback(self) -> Self;
}

impl<'w, 's, 'a> private::AddRollbackCommandExtensionSeal for EntityCommands<'w, 's, 'a> {}

impl<'w, 's, 'a> AddRollbackCommandExtension for EntityCommands<'w, 's, 'a> {
    fn add_rollback(mut self) -> Self {
        self.add(AddRollbackCommand);
        self
    }
}
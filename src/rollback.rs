use bevy::{prelude::{Component, Entity, World}, ecs::system::{EntityCommand, EntityCommands}};

/// This component flags an entity as being included in the rollback save/load schedule with GGRS.
/// 
/// You should use the `AddRollbackCommand` when spawning an entity, or provide the entity's ID via
/// the `Rollback::new(...)` method.
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Rollback(Entity);

impl Default for Rollback {
    fn default() -> Self {
        Self::new(Entity::from_raw(0))
    }
}

impl Rollback {
    /// Creates a new `Rollback` component from an `Entity`.
    pub fn new(entity: Entity) -> Self {
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
    fn add_rollback(&mut self) -> &mut Self;
}

impl<'w, 's, 'a> private::AddRollbackCommandExtensionSeal for EntityCommands<'w, 's, 'a> {}

impl<'w, 's, 'a> AddRollbackCommandExtension for EntityCommands<'w, 's, 'a> {
    fn add_rollback(&mut self) -> &mut Self {
        self.add(AddRollbackCommand)
    }
}
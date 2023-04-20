use bevy::{prelude::{Component, Entity, World}, ecs::system::EntityCommand};

/// Add this component to all entities you want to be loaded/saved on rollback.
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct RollbackFlag(Entity);

impl Default for RollbackFlag {
    fn default() -> Self {
        Self::new(Entity::from_raw(0))
    }
}

impl RollbackFlag {
    pub fn new(entity: Entity) -> Self {
        Self(entity)
    }
}

/// An `EntityCommand` which adds a `RollbackFlag` component to an entity.
pub struct Rollback;

impl EntityCommand for Rollback {
    fn write(self, id: Entity, world: &mut World) {
        world.entity_mut(id).insert(RollbackFlag::new(id));
    }
}
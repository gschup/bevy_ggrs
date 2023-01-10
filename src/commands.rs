use bevy::{
    ecs::system::{Command, EntityCommands},
    prelude::*,
};

use crate::{Rollback, RollbackIdProvider};

pub trait RollbackCommandsExt<'w, 's> {
    fn spawn_rollback<'a, T: Bundle>(&'a mut self, bundle: T) -> EntityCommands<'w, 's, 'a>;
}

impl<'w, 's> RollbackCommandsExt<'w, 's> for Commands<'w, 's> {
    fn spawn_rollback<'a, T: Bundle>(&'a mut self, bundle: T) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = self.spawn(bundle);
        entity_commands.insert_rollback();
        entity_commands
    }
}

pub trait RollbackEntityCommandsExt {
    fn insert_rollback(&mut self) -> &mut Self;
}

impl<'w, 's, 'a> RollbackEntityCommandsExt for EntityCommands<'w, 's, 'a> {
    fn insert_rollback(&mut self) -> &mut Self {
        let entity = self.id();
        self.commands().add(InsertRollback { entity });
        self
    }
}

struct InsertRollback {
    entity: Entity,
}

impl Command for InsertRollback {
    fn write(self, world: &mut World) {
        let mut rip = world.resource_mut::<RollbackIdProvider>();

        let insert = bevy::ecs::system::Insert {
            entity: self.entity,
            bundle: Rollback::new(rip.next_id()),
        };

        insert.write(world);
    }
}

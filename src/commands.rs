use bevy::{
    ecs::system::{Command, EntityCommands},
    prelude::*,
};

use crate::{Rollback, RollbackIdProvider};

pub trait RollbackCommandsExt<'w, 's> {
    /// Spawns a bundle and automatically inserts a [`Rollback`] component so
    /// the entity is tracked by `bevy_ggrs`.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_ggrs::RollbackCommandsExt;
    ///
    /// fn system_in_rollback_schedule(mut commands: Commands) {
    ///     commands.spawn_rollback(SpatialBundle::default());
    /// }
    /// ```
    ///
    /// This is an alternative to manually getting a rollback id and inserting
    /// it yourself:
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_ggrs::{RollbackIdProvider, Rollback};
    ///
    /// fn system_in_rollback_schedule(mut commands: Commands, mut rip: RollbackIdProvider) {
    ///     commands.spawn((
    ///         SpatialBundle::default(),
    ///         Rollback::new(rip.next_id())
    ///     ));
    /// }
    /// ```
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
    /// Inserts a rollback component on the entity so it's tracked by
    /// `bevy_ggrs`.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_ggrs::RollbackEntityCommandsExt;
    ///
    /// fn system_in_rollback_schedule(mut commands: Commands) {
    ///     commands.spawn(SpatialBundle::default()).insert_rollback();
    /// }
    /// ```
    ///
    /// See: [`RollbackCommandsExt::spawn_rollback`]
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

use bevy::{
    ecs::system::{EntityCommands, SystemParam},
    prelude::*,
};

use crate::RollbackIdProvider;

#[derive(SystemParam)]
pub struct RollbackCommands<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub rollback_id_provider: ResMut<'w, RollbackIdProvider>,
}

impl<'w, 's, 'a> RollbackCommands<'w, 's> {
    /// Spawns a new entity with the given components and tracks it with rollback
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_ggrs::RollbackCommands;
    ///
    /// fn system_in_rollback_schedule(mut commands: RollbackCommands) {
    ///     commands.spawn_rollback(SpatialBundle::default());
    /// }
    /// ```
    pub fn spawn_rollback<T: Bundle>(&'a mut self, bundle: T) -> EntityCommands<'w, 's, 'a> {
        let entity_commands = self.commands.spawn(bundle);
        entity_commands
    }
}

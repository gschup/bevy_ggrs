//! Rollback entity deferred despawning module.
//!
//! This module allows rollback entities to have non-rollback components (such as static collision
//! properties or mesh handles) by deferring the actual despawn until the requested frame has been
//! confirmed. Despawned entities are instead marked with the disabling component
//! [`RollbackDespawned`] so that they behave as though they are despawned by default.
//!
//! # Examples
//! ```rust
//! # use bevy::prelude::*;
//! # use bevy_ggrs::{prelude::*, ResourceChecksumPlugin};
//! #
//! # const FPS: usize = 60;
//! #
//! # type MyInputType = u8;
//! #
//! # fn read_local_inputs() {}
//! #
//! # fn start(session: Session<GgrsConfig<MyInputType>>) {
//! # let mut app = App::new();
//! #[derive(Resource, Clone, Hash)]
//! struct BossHealth(u32);
//!
//! // To include something in the checksum, it should also be rolled back
//! app.rollback_resource_with_clone::<BossHealth>();
//!
//! // This will update the checksum every frame to include BossHealth
//! app.add_plugins(ResourceChecksumPlugin::<BossHealth>::default());
//! # }
//! ```
//!
//! Entities which have been marked for despawn are disabled using the [`RollbackDespawned`]
//! component, so they will appear as though they are despawned to normal system queries.

use crate::snapshot::despawn::private::RollbackDespawnCommandExtensionSeal;
use crate::{
    AdvanceWorld, AdvanceWorldSystems, ConfirmedFrameCount, LoadWorld, LoadWorldSystems,
    RollbackFrameCount, SaveWorld, SaveWorldSystems,
};
use bevy::app::{App, Plugin};
use bevy::prelude::{
    Children, Component, Entity, EntityCommands, EntityMut, EntityRef, EntityWorldMut,
    IntoScheduleConfigs, Local, Query, QueryState, Res, World,
};
use ggrs::Frame;
use std::cmp::Ordering;

/// Marks an entity as despawned, contains the frame that the entity was despawned on.
///
/// When an entity is marked with this component, they MUST not be allowed to affect the simulation
/// in any way, as they may eventually be despawned on different frames for different peers.
/// This component is registered as "disabling" (see [ecs module docs](bevy_ecs::entity_disabling))
/// so that the entity will not show up in queries by default.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RollbackDespawned(Frame);

pub struct RollbackDespawnPlugin;

impl Plugin for RollbackDespawnPlugin {
    fn build(&self, app: &mut App) {
        app.register_disabling_component::<RollbackDespawned>();

        app.add_systems(
            LoadWorld,
            resurrect_entities.in_set(LoadWorldSystems::EntityResurrect),
        )
        .add_systems(
            AdvanceWorld,
            despawn_confirmed_entities.in_set(AdvanceWorldSystems::DespawnConfirmed),
        );
    }
}

fn resurrect_entities(
    world: &mut World,
    despawn_query: &mut QueryState<(Entity, &RollbackDespawned)>,
) {
    let rollback_frame = world.resource::<RollbackFrameCount>();

    despawn_query
        .iter(world)
        .filter_map(|(entity, despawned_frame)| {
            // During a world load, entities marked with the current rollback frame were marked
            // for despawn during that frame, so we only want to resurrect entities despawned after.
            Some(entity).filter(|_e| despawned_frame > rollback_frame)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|entity| {
            world.entity_mut(entity).remove::<RollbackDespawned>();
        });
}

fn despawn_confirmed_entities(
    world: &mut World,
    despawn_query: &mut QueryState<(Entity, &RollbackDespawned)>,
    mut local: Local<ConfirmedFrameCount>,
) {
    let confirmed_frame = world.resource::<ConfirmedFrameCount>();
    if *confirmed_frame == *local {
        return; // No work necessary
    }
    *local = *confirmed_frame;

    despawn_query
        .iter(world)
        .filter_map(|(entity, despawned_frame)| {
            // Entities marked as despawned on the confirmed frame or earlier can be immediately
            // despawned.
            Some(entity).filter(|_e| despawned_frame <= confirmed_frame)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|entity| {
            world.despawn(entity);
        });
}

mod private {
    pub trait RollbackDespawnCommandExtensionSeal {}
}
pub trait RollbackDespawnCommandExtension: private::RollbackDespawnCommandExtensionSeal {
    /// Despawns this entity and its children recursively using the [`RollbackDespawned`]
    /// component, such that they can be resurrected following a rollback.
    ///
    /// NOTE: This does not yet support [`RelationshipTarget`] with linked spawn mode.
    fn despawn_rollback(&mut self);

    /// NOTE: Not implemented yet.
    fn despawn_children_rollback(&mut self) -> &mut Self;

    /// NOTE: Not implemented yet.
    fn despawn_related_rollback<S>(&mut self) -> &mut Self;
}

impl RollbackDespawnCommandExtensionSeal for EntityCommands<'_> {}

impl RollbackDespawnCommandExtension for EntityCommands<'_> {
    fn despawn_rollback(&mut self) {
        self.queue_silenced(despawn_rollback);
    }

    fn despawn_children_rollback(&mut self) -> &mut Self {
        todo!()
    }

    fn despawn_related_rollback<S>(&mut self) -> &mut Self {
        todo!()
    }
}

fn despawn_rollback(mut entity: EntityWorldMut) {
    if let Some(&RollbackFrameCount(frame)) = entity.get_resource::<RollbackFrameCount>() {
        // If we have RollbackFrameCount we should also have ConfirmedFrameCount
        let &ConfirmedFrameCount(confirmed) = entity.get_resource::<ConfirmedFrameCount>().unwrap();

        // TODO handle wraparound
        if confirmed < frame {
            entity.insert_recursive::<Children>(RollbackDespawned(frame));
            return;
        }
    }

    // If current frame is confirmed or rollback sim is not present, we can simply despawn
    entity.despawn();
}

macro_rules! newtype_partial_ord {
    ($i:ident, $j:ident) => {
        impl PartialEq<$j> for $i {
            fn eq(&self, other: &$j) -> bool {
                self.0 == other.0
            }
        }

        impl PartialOrd<$j> for $i {
            fn partial_cmp(&self, other: &$j) -> Option<Ordering> {
                Some(self.0.cmp(&other.0))
            }
        }
    };
}

newtype_partial_ord!(RollbackDespawned, RollbackFrameCount);
newtype_partial_ord!(RollbackDespawned, ConfirmedFrameCount);

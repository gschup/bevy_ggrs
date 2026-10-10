//! Rollback entity deferred despawning module.
//!
//! A rollback respawns an entity the world lacks bare: [`RollbackId`](crate::RollbackId),
//! [`Rollback`](crate::Rollback) and the components registered for rollback. Everything else
//! the entity carried (static collision properties, mesh handles, a transform nothing in the
//! simulation writes) is gone for good, and nothing reports it. So a rollback entity is not
//! despawned while a snapshot that holds it can still be loaded. It is marked with the disabling
//! component [`RollbackDespawned`] instead, which hides it from every default query, and keeps
//! every component on it. [`LoadWorld`] lifts the mark where the loaded frame still had the
//! entity, and the mark becomes a despawn once no saved snapshot holds the entity.
//!
//! # Examples
//! ```rust
//! # use bevy::prelude::*;
//! # use bevy_ggrs::prelude::*;
//! #
//! fn despawn_player(
//!     mut commands: Commands,
//!     players: Query<Entity, With<Player>>,
//! ) {
//!     for entity in &players {
//!         // Instead of commands.entity(entity).despawn(), use despawn_rollback()
//!         // so the entity can be resurrected if a rollback happens.
//!         commands.entity(entity).despawn_rollback();
//!     }
//! }
//! # #[derive(Component)]
//! # struct Player;
//! ```
//!
//! Entities which have been marked for despawn are disabled using the [`RollbackDespawned`]
//! component, so they will appear as though they are despawned to normal system queries.

use crate::{
    ConfirmedFrameCount, GgrsComponentSnapshots, LoadWorld, LoadWorldSystems, RollbackFrameCount,
    RollbackId, SaveWorld, SaveWorldSystems, SnapshotFrames,
};
use bevy::app::{App, Plugin};
use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::*;
use ggrs::Frame;
use std::cmp::Ordering;

/// Marks an entity as despawned. Holds the first frame whose snapshot lacks the entity.
///
/// A [`LoadWorld`] of a frame before it lifts the mark; a load of that frame or a later one
/// keeps it. Once no saved snapshot holds the entity (its frames confirmed away, evicted, or
/// cleared by a session reset) it is despawned for good, at the end of the next [`SaveWorld`].
/// A second mark on an entity keeps the earlier frame: it says nothing new
/// about which snapshots hold the entity, and a later one would let a load between the two lift
/// the mark, after which the entity snapshot despawns the entity, since the frame lacks it.
///
/// [`despawn_rollback`](RollbackDespawnCommandExtension::despawn_rollback) reads the frame off
/// [`RollbackFrameCount`], which during [`AdvanceWorld`](crate::AdvanceWorld) names the frame being produced, the
/// first one whose snapshot lacks what the frame despawns. A runner whose counter means
/// something else passes the frame itself with
/// [`despawn_rollback_since`](RollbackDespawnCommandExtension::despawn_rollback_since).
///
/// When an entity is marked with this component, they MUST not be allowed to affect the simulation
/// in any way, as they may eventually be despawned on different frames for different peers.
/// This component is registered as "disabling" (see [ecs module docs](bevy_ecs::entity_disabling))
/// so that the entity will not show up in queries by default.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RollbackDespawned(Frame);

impl RollbackDespawned {
    /// A mark naming `frame` as the first frame whose snapshot lacks the entity.
    pub fn since(frame: Frame) -> Self {
        Self(frame)
    }

    /// The first frame whose snapshot lacks the entity.
    pub fn frame(&self) -> Frame {
        self.0
    }
}

/// Plugin that enables rollback-safe despawning via [`RollbackDespawned`].
///
/// Registers [`RollbackDespawned`] as a disabling component and installs the systems that
/// resurrect entities during rollback and permanently despawn them once no saved snapshot holds
/// them.
pub struct RollbackDespawnPlugin;

impl Plugin for RollbackDespawnPlugin {
    fn build(&self, app: &mut App) {
        app.register_disabling_component::<RollbackDespawned>();

        app.add_systems(
            LoadWorld,
            resurrect_entities.in_set(LoadWorldSystems::EntityResurrect),
        )
        .add_systems(
            SaveWorld,
            despawn_unheld_entities.after(SaveWorldSystems::Snapshot),
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
            // The mark names the first frame whose snapshot lacks the entity, so a load of
            // that frame keeps it and only a load of an earlier frame lifts it.
            Some(entity).filter(|_e| despawned_frame > rollback_frame)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|entity| {
            world.entity_mut(entity).remove::<RollbackDespawned>();
        });
}

/// Despawn every marked rollback entity no saved snapshot holds.
///
/// After the save, so the frames discarded this [`SaveWorld`] (confirmed away, beyond the
/// depth, or cleared by a session reset) count as gone. A mark names the first frame whose
/// snapshot lacks the entity, and the entity is in every frame from its spawn up to that one,
/// so the newest saved frame before the mark holds it or no saved frame does: one lookup per
/// mark. A marked entity without a [`RollbackId`] is a descendant marked with its rollback
/// ancestor and goes with it.
fn despawn_unheld_entities(
    world: &mut World,
    marked: &mut QueryState<(Entity, &RollbackId, &RollbackDespawned)>,
) {
    let frames = world.resource::<SnapshotFrames>();
    let entities = world.resource::<GgrsComponentSnapshots<Entity>>();
    let unheld: Vec<Entity> = marked
        .iter(world)
        .filter(|(_, rollback, mark)| {
            !frames
                .newest_before(mark.frame())
                .and_then(|frame| entities.peek(frame))
                .is_some_and(|snapshot| snapshot.get(rollback).is_some())
        })
        .map(|(entity, _, _)| entity)
        .collect();
    for entity in unheld {
        // A descendant may have gone with its ancestor already.
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
}

/// Extension trait for [`EntityCommands`] that adds rollback-safe despawn methods.
pub trait RollbackDespawnCommandExtension {
    /// Despawns this entity and its children recursively using the [`RollbackDespawned`]
    /// component, such that they can be resurrected following a rollback.
    ///
    /// The mark names the current [`RollbackFrameCount`], which during
    /// [`AdvanceWorld`](crate::AdvanceWorld) is the
    /// frame being produced: the first frame whose snapshot lacks the entity.
    ///
    /// NOTE: This does not yet support [`RelationshipTarget`] with linked spawn mode.
    fn despawn_rollback(&mut self);

    /// [`despawn_rollback`](Self::despawn_rollback) with the frame given: the first frame whose
    /// snapshot lacks the entity. For a runner whose [`RollbackFrameCount`] does not name that
    /// frame at the time of the despawn.
    fn despawn_rollback_since(&mut self, frame: Frame);
}

impl RollbackDespawnCommandExtension for EntityCommands<'_> {
    fn despawn_rollback(&mut self) {
        self.queue_silenced(despawn_rollback);
    }

    fn despawn_rollback_since(&mut self, frame: Frame) {
        self.queue_silenced(move |entity: EntityWorldMut| despawn_rollback_since(entity, frame));
    }
}

/// [`RollbackDespawnCommandExtension::despawn_rollback`] on an [`EntityWorldMut`].
pub fn despawn_rollback(entity: EntityWorldMut) {
    match entity.get_resource::<RollbackFrameCount>() {
        Some(&RollbackFrameCount(frame)) => despawn_rollback_since(entity, frame),
        // No rollback sim: nothing can ask for the entity back.
        None => entity.despawn(),
    }
}

/// [`RollbackDespawnCommandExtension::despawn_rollback_since`] on an [`EntityWorldMut`]: mark
/// the entity and its descendants as despawned since `frame`, or despawn them outright when
/// `frame` is confirmed already. A missing [`ConfirmedFrameCount`] confirms nothing.
pub fn despawn_rollback_since(mut entity: EntityWorldMut, frame: Frame) {
    let confirmed = entity
        .get_resource::<ConfirmedFrameCount>()
        .map(|&ConfirmedFrameCount(confirmed)| confirmed);
    // TODO handle wraparound
    if confirmed.is_some_and(|confirmed| confirmed >= frame) {
        entity.despawn();
        return;
    }
    let root = entity.id();
    entity.world_scope(|world| {
        let mut pending = vec![root];
        while let Some(entity) = pending.pop() {
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            let since = world
                .get::<RollbackDespawned>(entity)
                .map_or(frame, |marked| marked.0.min(frame));
            world
                .entity_mut(entity)
                .insert(RollbackDespawned::since(since));
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{
        PinnedFrames, Rollback, RollbackId, SnapshotPlugin,
        tests::{advance_frame, load_world, save_world},
    };
    use bevy::ecs::query::Allow;

    /// A component nothing registers for rollback: what a bare respawn would lose.
    #[derive(Component, Debug, PartialEq)]
    struct Authored(u32);

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SnapshotPlugin);
        app.finish();
        app.cleanup();
        app.update();
        app
    }

    /// Spawn a rollback entity with a child, save frames 0..=2, and mark it since 2.
    fn marked_at_two(app: &mut App) -> (Entity, Entity) {
        let world = app.world_mut();
        let child = world.spawn((Rollback, Authored(7))).id();
        let entity = world.spawn((Rollback, Authored(1))).add_child(child).id();
        world.flush();
        save_world(world); // frame 0
        advance_frame(world);
        save_world(world); // frame 1
        advance_frame(world);
        // Frame 2 is produced now; its snapshot will lack the entity.
        despawn_rollback(world.entity_mut(entity));
        save_world(world); // frame 2
        (entity, child)
    }

    fn visible(world: &mut World, entity: Entity) -> bool {
        world
            .query_filtered::<Entity, With<RollbackId>>()
            .iter(world)
            .any(|e| e == entity)
    }

    #[test]
    fn a_mark_hides_the_entity_and_its_children_and_keeps_their_components() {
        let mut app = app();
        let (entity, child) = marked_at_two(&mut app);
        let world = app.world_mut();
        assert_eq!(
            world.get::<RollbackDespawned>(entity),
            Some(&RollbackDespawned::since(2))
        );
        assert_eq!(
            world.get::<RollbackDespawned>(child),
            Some(&RollbackDespawned::since(2))
        );
        assert!(!visible(world, entity));
        assert!(!visible(world, child));
        assert_eq!(world.get::<Authored>(entity), Some(&Authored(1)));
        assert_eq!(world.get::<Authored>(child), Some(&Authored(7)));
    }

    #[test]
    fn a_load_before_the_mark_lifts_it_whole() {
        let mut app = app();
        let (entity, child) = marked_at_two(&mut app);
        let world = app.world_mut();
        load_world(world, 1);
        assert!(visible(world, entity), "the same entity, back");
        assert!(visible(world, child));
        assert_eq!(world.get::<Authored>(entity), Some(&Authored(1)));
        assert_eq!(world.get::<Authored>(child), Some(&Authored(7)));
        assert_eq!(
            world
                .query_filtered::<Entity, With<RollbackId>>()
                .iter(world)
                .count(),
            2,
            "nothing respawned beside it"
        );
    }

    #[test]
    fn a_load_at_the_mark_keeps_it() {
        let mut app = app();
        let (entity, _) = marked_at_two(&mut app);
        let world = app.world_mut();
        load_world(world, 2);
        assert!(!visible(world, entity));
        assert!(world.get_entity(entity).is_ok(), "marked, not despawned");
    }

    #[test]
    fn a_second_mark_keeps_the_earlier_frame() {
        let mut app = app();
        let (entity, child) = marked_at_two(&mut app);
        let world = app.world_mut();
        despawn_rollback_since(world.entity_mut(entity), 5);
        assert_eq!(
            world.get::<RollbackDespawned>(entity),
            Some(&RollbackDespawned::since(2))
        );
        assert_eq!(
            world.get::<RollbackDespawned>(child),
            Some(&RollbackDespawned::since(2))
        );
        despawn_rollback_since(world.entity_mut(entity), 1);
        assert_eq!(
            world.get::<RollbackDespawned>(entity),
            Some(&RollbackDespawned::since(1))
        );
    }

    #[test]
    fn a_mark_no_saved_snapshot_holds_is_despawned() {
        let mut app = app();
        let (entity, child) = marked_at_two(&mut app);
        let world = app.world_mut();
        world.insert_resource(ConfirmedFrameCount(1));
        advance_frame(world);
        save_world(world); // frame 3; frame 0 confirmed away, frame 1 still holds it
        assert!(world.get_entity(entity).is_ok(), "frame 1 holds it");
        world.insert_resource(ConfirmedFrameCount(2));
        advance_frame(world);
        save_world(world); // frame 4; frame 1 confirmed away
        assert!(world.get_entity(entity).is_err());
        assert!(world.get_entity(child).is_err());
    }

    #[test]
    fn a_pinned_frame_that_holds_it_keeps_the_mark() {
        let mut app = app();
        let (entity, _) = marked_at_two(&mut app);
        let world = app.world_mut();
        world.resource_mut::<PinnedFrames>().insert(1);
        world.insert_resource(ConfirmedFrameCount(2));
        advance_frame(world);
        save_world(world); // frames 0 confirmed away; 1 pinned
        assert!(world.get_entity(entity).is_ok(), "pinned frame 1 holds it");
        assert_eq!(
            world.get::<RollbackDespawned>(entity),
            Some(&RollbackDespawned::since(2))
        );
    }

    #[test]
    fn a_session_reset_despawns_every_mark() {
        let mut app = app();
        let (entity, _) = marked_at_two(&mut app);
        let world = app.world_mut();
        world.trigger(crate::ClearSnapshots);
        world.resource_mut::<RollbackFrameCount>().0 = 0;
        save_world(world);
        assert!(world.get_entity(entity).is_err());
    }

    #[test]
    fn a_frame_already_confirmed_despawns_outright() {
        let mut app = app();
        let world = app.world_mut();
        let entity = world.spawn(Rollback).id();
        world.flush();
        world.insert_resource(ConfirmedFrameCount(3));
        despawn_rollback_since(world.entity_mut(entity), 3);
        assert!(world.get_entity(entity).is_err());
    }

    #[test]
    fn no_confirmed_frame_confirms_nothing() {
        let mut app = app();
        let world = app.world_mut();
        world.remove_resource::<ConfirmedFrameCount>();
        let entity = world.spawn(Rollback).id();
        world.flush();
        despawn_rollback_since(world.entity_mut(entity), 0);
        assert_eq!(
            world.get::<RollbackDespawned>(entity),
            Some(&RollbackDespawned::since(0))
        );
    }

    #[test]
    fn a_query_may_ask_for_marked_entities() {
        let mut app = app();
        let (entity, _) = marked_at_two(&mut app);
        let world = app.world_mut();
        let seen: Vec<Entity> = world
            .query_filtered::<Entity, (With<RollbackId>, Allow<RollbackDespawned>)>()
            .iter(world)
            .collect();
        assert!(seen.contains(&entity));
    }
}

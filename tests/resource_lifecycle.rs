//! Tests for resources that are inserted or removed during a session, and for resource
//! entity-reference remapping via `MapEntities`.
//!
//! The resource snapshot system stores `None` when a resource is absent, so rollback must
//! correctly insert/remove the resource when loading a snapshot from across the transition
//! frame. These tests verify both directions of that transition, plus that `Entity` references
//! inside resources are remapped correctly after rollback re-creates entities with new IDs.
//!
//! Detection strategy: a `FrameLog` resource (always present, checksummed) accumulates
//! differently depending on whether the optional resource exists. If rollback incorrectly
//! preserves or drops the optional resource, `FrameLog` diverges during re-simulation and
//! `SyncTestMismatch` fires.

#[allow(dead_code)]
mod common;
use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

/// The optional resource that transitions between present and absent mid-session.
#[derive(Resource, Clone, Debug, PartialEq)]
struct Wallet(u32);

/// Always-present resource whose value depends on `Wallet`'s presence each frame.
/// Checksumming this detects any incorrect rollback of `Wallet`.
#[derive(Resource, Clone, Hash, Default, Debug)]
struct FrameLog(u32);

/// Tracks whether Wallet is present each frame; accumulates 2 when present, 1 when absent.
fn track_wallet(mut log: ResMut<FrameLog>, wallet: Option<Res<Wallet>>) {
    log.0 = log.0.wrapping_add(if wallet.is_some() { 2 } else { 1 });
}

/// Verifies that a resource inserted mid-session is correctly removed when rolling back
/// to a frame before its insertion.
///
/// `Wallet` is absent at the start and inserted at frame 3. With `check_distance = 4`,
/// the SyncTest rolls back across the insertion frame every time it reaches frame 7+,
/// re-simulating the insertion. If rollback fails to remove `Wallet` before frame 3,
/// `FrameLog` will accumulate a different value during re-simulation → `SyncTestMismatch`.
#[test]
fn resource_inserted_mid_session_rolls_back() {
    fn insert_wallet_at_frame_3(mut commands: Commands, frame: Res<RollbackFrameCount>) {
        if frame.0 == 3 {
            commands.insert_resource(Wallet(100));
        }
    }

    let mut app = base_synctest_app(4);
    app.init_resource::<FrameLog>()
        .rollback_resource_with_clone::<Wallet>()
        .rollback_resource_with_clone::<FrameLog>()
        .checksum_resource_with_hash::<FrameLog>()
        .add_systems(
            GgrsSchedule,
            (insert_wallet_at_frame_3, track_wallet).chain(),
        );

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: Wallet rollback (insert) is non-deterministic");
    });

    for _ in 0..20 {
        app.update();
    }

    assert!(
        app.world().get_resource::<Wallet>().is_some(),
        "Wallet should be present after frame 3"
    );
    assert_eq!(
        app.world().resource::<Wallet>().0,
        100,
        "Wallet value should be unchanged"
    );
}

/// Verifies that a resource removed mid-session is correctly re-inserted when rolling back
/// to a frame before its removal.
///
/// `Wallet` starts present (value 100) and is removed at frame 3. With `check_distance = 4`,
/// the SyncTest rolls back across the removal frame every time it reaches frame 7+,
/// re-simulating the removal. If rollback fails to re-insert `Wallet` before frame 3,
/// `FrameLog` will accumulate a different value during re-simulation → `SyncTestMismatch`.
#[test]
fn resource_removed_mid_session_rolls_back() {
    fn remove_wallet_at_frame_3(mut commands: Commands, frame: Res<RollbackFrameCount>) {
        if frame.0 == 3 {
            commands.remove_resource::<Wallet>();
        }
    }

    let mut app = base_synctest_app(4);
    app.insert_resource(Wallet(100))
        .init_resource::<FrameLog>()
        .rollback_resource_with_clone::<Wallet>()
        .rollback_resource_with_clone::<FrameLog>()
        .checksum_resource_with_hash::<FrameLog>()
        .add_systems(
            GgrsSchedule,
            (remove_wallet_at_frame_3, track_wallet).chain(),
        );

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: Wallet rollback (remove) is non-deterministic");
    });

    for _ in 0..20 {
        app.update();
    }

    assert!(
        app.world().get_resource::<Wallet>().is_none(),
        "Wallet should be absent after being removed at frame 3"
    );
}

/// Verifies that `update_resource_with_map_entities` is correctly wired into the rollback
/// pipeline: the system runs on every rollback restore and the entity reference in the
/// resource remains valid (not stale) throughout the session.
///
/// A `Target(Entity)` resource holds a reference to a long-lived rollback entity. An
/// `AtomicBool` inside the `MapEntities` implementation records whether the mapping system
/// was ever invoked. After enough updates to trigger multiple rollbacks the entity reference
/// must still point to a valid entity, confirming the mapping is applied correctly.
#[test]
fn resource_map_entities_remaps_after_rollback() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static MAP_ENTITIES_CALLED: AtomicBool = AtomicBool::new(false);

    #[derive(Component, Copy, Clone, Hash)]
    struct Tracked;

    // Resource holding an entity reference that must survive rollbacks intact.
    #[derive(Resource, Clone)]
    struct Target(Entity);

    impl MapEntities for Target {
        fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
            MAP_ENTITIES_CALLED.store(true, Ordering::SeqCst);
            self.0 = entity_mapper.get_mapped(self.0);
        }
    }

    let mut app = base_synctest_app(2);

    // Spawn a long-lived rollback entity and store its ID in the resource.
    let entity = app.world_mut().spawn((Tracked, Rollback)).id();
    app.world_mut().insert_resource(Target(entity));

    app.rollback_component_with_copy::<Tracked>()
        .rollback_resource_with_clone::<Target>()
        .update_resource_with_map_entities::<Target>();

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: MapEntities resource test is non-deterministic");
    });

    for _ in 0..20 {
        app.update();
    }

    assert!(
        MAP_ENTITIES_CALLED.load(Ordering::SeqCst),
        "MapEntities should have been called at least once during rollback"
    );

    let target_entity = app.world().resource::<Target>().0;
    assert!(
        app.world().get_entity(target_entity).is_ok(),
        "Target entity reference should still be valid after all rollbacks"
    );
}

/// Verifies that a resource a component hook writes while a rollback respawns an entity
/// ends up at its snapshot value.
///
/// `Ticket`'s insert hook takes a number from the rolled-back `Issued` counter, the way
/// an id allocator does. The entity is despawned at frame 3, so with `check_distance = 4`
/// every rollback across that frame respawns it, and the restore inserts `Ticket` again.
/// Resources are restored after that insert, so the hook's increment is overwritten; were
/// they restored first, `Issued` would gain one per respawn and `SyncTestMismatch` fires.
#[test]
fn resource_written_by_a_hook_during_respawn_rolls_back() {
    use bevy::ecs::lifecycle::HookContext;
    use bevy::ecs::world::DeferredWorld;

    #[derive(Component, Copy, Clone, Hash)]
    #[component(on_insert = take_ticket)]
    struct Ticket;

    #[derive(Resource, Copy, Clone, Hash, Default)]
    struct Issued(u32);

    fn take_ticket(mut world: DeferredWorld, _: HookContext) {
        world.resource_mut::<Issued>().0 += 1;
    }

    fn despawn_at_frame_3(
        mut commands: Commands,
        frame: Res<RollbackFrameCount>,
        tickets: Query<Entity, With<Ticket>>,
    ) {
        if frame.0 == 3 {
            for entity in &tickets {
                commands.entity(entity).despawn();
            }
        }
    }

    let mut app = base_synctest_app(4);
    app.init_resource::<Issued>()
        .rollback_component_with_copy::<Ticket>()
        .rollback_resource_with_copy::<Issued>()
        .checksum_resource_with_hash::<Issued>()
        .add_systems(GgrsSchedule, despawn_at_frame_3);
    app.world_mut().spawn((Ticket, Rollback));

    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: a hook's write to Issued survived the rollback");
    });

    for _ in 0..20 {
        app.update();
    }

    assert!(app.world().resource::<RollbackFrameCount>().0 > 7);
    assert_eq!(
        app.world().resource::<Issued>().0,
        1,
        "one Ticket was ever inserted outside a rollback"
    );
}

/// Verifies that a component hook that runs while a rollback respawns an entity reads a
/// rolled-back resource as the snapshot has it.
///
/// `Clock` holds the frame it was last advanced on. `Probe`'s insert hook records
/// `(RollbackFrameCount, Clock)` in a resource nothing rolls back. The entity is despawned at
/// frame 3, so with `check_distance = 4` every rollback across that frame respawns it. Each
/// record from a respawn must pair the loaded frame with that frame's `Clock`; a resource
/// restored only after the hooks would show the `Clock` of the frame the rollback started from.
#[test]
fn a_hook_during_respawn_reads_resources_as_the_snapshot_has_them() {
    use bevy::ecs::lifecycle::HookContext;
    use bevy::ecs::world::DeferredWorld;

    #[derive(Component, Copy, Clone, Hash)]
    #[component(on_insert = record_clock)]
    struct Probe;

    #[derive(Resource, Copy, Clone, Hash, Default)]
    struct Clock(i32);

    #[derive(Resource, Default)]
    struct Records(Vec<(i32, i32)>);

    fn record_clock(mut world: DeferredWorld, _: HookContext) {
        let frame = world.resource::<RollbackFrameCount>().0;
        let clock = world.resource::<Clock>().0;
        world.resource_mut::<Records>().0.push((frame, clock));
    }

    fn advance_clock(mut clock: ResMut<Clock>, frame: Res<RollbackFrameCount>) {
        clock.0 = frame.0;
    }

    fn despawn_at_frame_3(
        mut commands: Commands,
        frame: Res<RollbackFrameCount>,
        probes: Query<Entity, With<Probe>>,
    ) {
        if frame.0 == 3 {
            for entity in &probes {
                commands.entity(entity).despawn();
            }
        }
    }

    let mut app = base_synctest_app(4);
    app.init_resource::<Clock>()
        .init_resource::<Records>()
        .rollback_component_with_copy::<Probe>()
        .rollback_resource_with_copy::<Clock>()
        .add_systems(GgrsSchedule, (advance_clock, despawn_at_frame_3).chain());
    app.world_mut().spawn((Probe, Rollback));

    for _ in 0..20 {
        app.update();
    }

    let records = &app.world().resource::<Records>().0;
    assert!(records.len() > 1, "the rollbacks respawned the entity");
    let stale: Vec<_> = records.iter().filter(|(f, c)| f != c).collect();
    assert!(
        stale.is_empty(),
        "a hook read Clock from another frame than the one loaded: {records:?}"
    );
}

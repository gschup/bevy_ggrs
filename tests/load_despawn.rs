//! A load that drops a rollback entity together with its rollback child.
//!
//! Its own binary: it counts warnings with a `log` logger, which is global
//! to the process, and a test elsewhere that adds `LogPlugin` installs one
//! first. Bevy reports a failed `despawn` through `log` with the `warn`
//! handler fixed, so neither `FallbackErrorHandler` nor a tracing
//! subscriber sees it.

use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;
use bevy_ggrs::*;

static WARNINGS: AtomicU32 = AtomicU32::new(0);

struct Warnings;

impl log::Log for Warnings {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Warn {
            WARNINGS.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {}
}

fn run_at(world: &mut World, frame: i32, schedule: impl bevy::ecs::schedule::ScheduleLabel) {
    world.resource_mut::<RollbackFrameCount>().0 = frame;
    world.run_schedule(schedule);
}

/// Each entity goes once and nothing warns: the parent's despawn takes the
/// child with it, which the child's own despawn then finds gone.
#[test]
fn a_load_despawns_a_rollback_child_with_its_parent_without_a_warning() {
    log::set_logger(&Warnings).expect("this binary owns the logger");
    log::set_max_level(log::LevelFilter::Warn);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(SnapshotPlugin);
    app.finish();
    app.cleanup();
    app.update();
    let world = app.world_mut();
    run_at(world, 0, SaveWorld); // before the families
    // The load despawns in its map's fixed hash order, which reaches a
    // parent before its child in three of these eight.
    for _ in 0..8 {
        world.spawn(Rollback).with_children(|parent| {
            parent.spawn(Rollback);
        });
    }
    world.flush();
    run_at(world, 1, SaveWorld);

    WARNINGS.store(0, Ordering::Relaxed);
    run_at(world, 0, LoadWorld);
    assert_eq!(
        world
            .query_filtered::<Entity, With<RollbackId>>()
            .iter(world)
            .count(),
        0,
        "the families are gone"
    );
    assert_eq!(WARNINGS.load(Ordering::Relaxed), 0, "nothing warned");
}

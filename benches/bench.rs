use bevy::MinimalPlugins;
use bevy::ecs::component::Mutable;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_ggrs::SaveWorld;
use bevy_ggrs::SnapshotPlugin;
use bevy_ggrs::{AdvanceWorld, LoadWorld, RollbackFrameCount, prelude::*};
use criterion::{Criterion, criterion_group, criterion_main};

#[derive(Component, Clone, Copy)]
struct Foo(i32);

#[derive(Component, Clone, Copy)]
struct Bar(i32);

#[derive(Component, Clone, Copy)]
struct Baz(i32);

fn advance_and_load(app: &mut App) {
    app.world_mut().run_schedule(AdvanceWorld);
    app.insert_resource(RollbackFrameCount(0));
    app.world_mut().run_schedule(LoadWorld);
}

fn advance_and_save(app: &mut App) {
    app.world_mut().run_schedule(AdvanceWorld);
    app.world_mut().run_schedule(SaveWorld);
}

fn increment_foos(mut foos: Query<&mut Foo>) {
    for mut component in &mut foos {
        component.0 += 1;
    }
}

fn decrement_bars(mut bars: Query<&mut Bar>) {
    for mut component in &mut bars {
        component.0 -= 1;
    }
}

fn increment_bazs(mut bazs: Query<&mut Baz>) {
    for mut component in &mut bazs {
        component.0 += 1;
    }
}

fn foo_1000(c: &mut Criterion) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SnapshotPlugin));
    app.add_systems(AdvanceWorld, increment_foos);
    app.rollback_component_with_copy::<Foo>();
    app.update();
    app.world_mut()
        .run_system_once(|mut commands: Commands| {
            for i in 0..1000 {
                commands.spawn((Foo(i), Rollback));
            }
        })
        .unwrap();
    app.world_mut().run_schedule(SaveWorld);
    c.bench_function("advance_and_load_1000_components", |b| {
        b.iter(|| advance_and_load(&mut app))
    });
    c.bench_function("advance_and_save_1000_components", |b| {
        b.iter(|| advance_and_save(&mut app))
    });
}

fn foo_bar_baz_1000(c: &mut Criterion) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SnapshotPlugin));
    app.add_systems(AdvanceWorld, increment_foos);
    app.add_systems(AdvanceWorld, decrement_bars);
    app.add_systems(AdvanceWorld, increment_bazs);
    app.rollback_component_with_copy::<Foo>();
    app.rollback_component_with_copy::<Bar>();
    app.rollback_component_with_copy::<Baz>();
    app.update();
    app.world_mut()
        .run_system_once(|mut commands: Commands| {
            for i in 0..1000 {
                commands.spawn((Foo(i), Rollback));
                commands.spawn((Bar(i), Rollback));
                commands.spawn((Baz(i), Rollback));
            }
        })
        .unwrap();
    app.world_mut().run_schedule(SaveWorld);
    c.bench_function("advance_and_load_3000_disjoint_components", |b| {
        b.iter(|| advance_and_load(&mut app))
    });
    c.bench_function("advance_and_save_3000_disjoint_components", |b| {
        b.iter(|| advance_and_save(&mut app))
    });
}

/// Keeps a summary tick per table, so a save skips its tables when they did
/// not change.
#[derive(Component, Clone, Copy, Hash)]
#[component(summary_tick)]
struct Still(i32);

/// The same without a summary tick: a save reads every change tick.
#[derive(Component, Clone, Copy, Hash)]
struct Unmarked(i32);

/// A save of a world where nothing changed, with `n` rollback entities of
/// `C`, snapshotted and checksummed: with a summary tick its cost should not
/// follow `n`.
fn save_unchanged<C: Component<Mutability = Mutable> + Copy + std::hash::Hash>(
    c: &mut Criterion,
    name: &str,
    n: i32,
    make: fn(i32) -> C,
) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SnapshotPlugin));
    app.rollback_component_with_copy::<C>();
    app.checksum_component_with_hash::<C>();
    app.update();
    app.world_mut()
        .run_system_once(move |mut commands: Commands| {
            for i in 0..n {
                commands.spawn((make(i), Rollback));
            }
        })
        .unwrap();
    app.world_mut().run_schedule(SaveWorld);
    // A frame a save, as the runner saves: the prune of `RollbackOrdered`
    // then runs every 128 saves, not on each.
    c.bench_function(&format!("save_unchanged_{name}_{n}"), |b| {
        b.iter(|| {
            app.world_mut().resource_mut::<RollbackFrameCount>().0 += 1;
            app.world_mut().run_schedule(SaveWorld);
        })
    });
}

fn unchanged(c: &mut Criterion) {
    for n in [0, 4_000, 40_000] {
        save_unchanged(c, "summary_tick", n, Still);
        save_unchanged(c, "no_summary_tick", n, Unmarked);
    }
}

criterion_group!(benches, foo_1000, foo_bar_baz_1000, unchanged);
criterion_main!(benches);

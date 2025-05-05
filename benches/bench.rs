use bevy::MinimalPlugins;
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
    for mut foo in &mut foos {
        foo.0 += 1;
    }
}

fn decrement_bars(mut bars: Query<&mut Bar>) {
    for mut bar in &mut bars {
        bar.0 -= 1;
    }
}

fn increment_bazs(mut bazs: Query<&mut Baz>) {
    for mut baz in &mut bazs {
        baz.0 += 1;
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
                commands.spawn(Foo(i)).add_rollback();
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
                commands.spawn(Foo(i)).add_rollback();
                commands.spawn(Bar(i)).add_rollback();
                commands.spawn(Baz(i)).add_rollback();
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

criterion_group!(benches, foo_1000, foo_bar_baz_1000);
criterion_main!(benches);

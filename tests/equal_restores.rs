//! A component registered with an `_eq` strategy is left alone by a load
//! when it already holds the snapshot's bits, so it is not marked changed;
//! one that differs is written. Equal means the same bits: `-0.0` is written
//! over `0.0`, which `PartialEq` calls equal.

#[allow(dead_code)]
mod common;
use bevy::prelude::*;
use bevy_ggrs::{prelude::*, *};
use common::base_synctest_app;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Value(f32);

impl RollbackEq for Value {
    fn rollback_eq(&self, other: &Self) -> bool {
        self.0.rollback_eq(&other.0)
    }
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

#[derive(Component)]
struct Moves;

/// How many changes of each entity's `Value` an `Update` system has seen.
#[derive(Resource, Default)]
struct Seen {
    still: u32,
    moving: u32,
}

/// The moving entity flips between `0.0` and `-0.0` every frame: equal by
/// `PartialEq`, different by bits.
fn flip(mut moving: Query<&mut Value, With<Moves>>) {
    for mut value in &mut moving {
        value.0 = if value.0.is_sign_negative() { 0.0 } else { -0.0 };
    }
}

#[test]
fn a_load_writes_only_what_differs_by_bits() {
    let mut app = base_synctest_app(3);
    app.init_resource::<Seen>()
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Value(1.5), Rollback));
            commands.spawn((Value(0.0), Moves, Rollback));
        })
        .rollback_component_with_copy_eq::<Value>()
        .checksum_component_with_hash::<Value>()
        .add_systems(GgrsSchedule, flip)
        .add_systems(
            Update,
            |values: Query<(Ref<Value>, Has<Moves>)>, mut seen: ResMut<Seen>| {
                for (value, moves) in &values {
                    if value.is_changed() {
                        if moves {
                            seen.moving += 1;
                        } else {
                            seen.still += 1;
                        }
                    }
                }
            },
        );
    app.world_mut().add_observer(|_: On<SyncTestMismatch>| {
        panic!("SyncTestMismatch: an equal-looking value was left with other bits");
    });
    for _ in 0..30 {
        app.update();
    }
    let seen = app.world().resource::<Seen>();
    assert_eq!(seen.still, 1, "the still one changed at spawn only");
    assert!(seen.moving > 20, "the flipping one is written: {}", seen.moving);
}

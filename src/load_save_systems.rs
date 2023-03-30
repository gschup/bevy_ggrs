use bevy::prelude::*;

use crate::{
    world_snapshot::{RollbackSnapshots, WorldSnapshot},
    RollbackFrameCount, RollbackTypeRegistry,
};

pub fn save_world(world: &mut World) {
    // we make a snapshot of our world
    let rollback_registry = world
        .remove_resource::<RollbackTypeRegistry>()
        .expect("GGRS type registry not found. Did you remove it?");
    let snapshot = WorldSnapshot::from_world(world, &rollback_registry.0);
    world.insert_resource(rollback_registry);

    let frame = world
        .get_resource_mut::<RollbackFrameCount>()
        .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
        .0;

    let mut snapshots = world
        .get_resource_mut::<RollbackSnapshots>()
        .expect("No GGRS RollbackSnapshots resource found. Did you remove it?");

    // store the snapshot ourselves (since the snapshots don't implement clone)
    let pos = frame as usize % snapshots.0.len();
    snapshots.0[pos] = snapshot;
}

pub fn load_world(world: &mut World) {
    let frame = world
        .get_resource_mut::<RollbackFrameCount>()
        .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
        .0;

    let rollback_registry = world
        .remove_resource::<RollbackTypeRegistry>()
        .expect("GGRS type registry not found. Did you remove it?");

    let snapshots = world
        .remove_resource::<RollbackSnapshots>()
        .expect("No GGRS RollbackSnapshots resource found. Did you remove it?");

    // we get the correct snapshot
    let pos = frame as usize % snapshots.0.len();
    let snapshot_to_load = &snapshots.0[pos];

    // load the entities
    snapshot_to_load.write_to_world(world, &rollback_registry.0);

    world.insert_resource(rollback_registry);
    world.insert_resource(snapshots);
}

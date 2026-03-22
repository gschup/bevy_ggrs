#[allow(dead_code)]
mod common;
use bevy::{platform::collections::HashMap, prelude::*, time::TimeUpdateStrategy};
use bevy_ggrs::*;
use common::{GgrsConfig, synctest_session};
use core::time::Duration;

#[derive(Reflect, Component, Default)]
struct ChildEntity;

#[derive(Reflect, Component, Default)]
struct ParentEntity;

#[derive(Reflect, Resource, Default, Debug)]
struct FrameCounter(u16);

fn input_system(
    mut commands: Commands,
    mut delete_events: MessageReader<DeleteChildEntityMessage>,
) {
    let should_delete = u8::from(delete_events.read().count() > 0);

    let mut local_inputs = HashMap::new();
    local_inputs.insert(0, should_delete);

    commands.insert_resource(LocalInputs::<GgrsConfig>(local_inputs));
}

fn setup_system(mut commands: Commands) {
    commands
        .spawn((ParentEntity, Rollback))
        .with_children(|parent| {
            parent.spawn((ChildEntity, Rollback));
        });
}

fn delete_child_system(
    mut commands: Commands,
    inputs: Res<PlayerInputs<GgrsConfig>>,
    parent: Query<&Children, With<ParentEntity>>,
    child: Query<Entity, With<ChildEntity>>,
) {
    println!("Inputs: {:?}", **inputs);

    println!("Parent's children: {:?}", parent.single());

    if let Ok(child) = child.single() {
        println!("Child exists: {child:?}");
    }

    if inputs[0].0 == 1 {
        println!("Despawning child");
        let child_entity = parent.single().unwrap()[0];
        commands.entity(child_entity).despawn();
    }
}

fn frame_counter(mut counter: ResMut<FrameCounter>) {
    println!("==== Frame {} ====", counter.0);
    counter.0 = counter.0.wrapping_add(1);
}

#[derive(Message)]
struct DeleteChildEntityMessage;

/// Verifies that a 3-level parent→child→grandchild hierarchy is fully preserved through rollback.
///
/// All three entities carry `Rollback`. After running enough updates to trigger several
/// rollbacks, each level of the hierarchy should still be intact (correct `ChildOf` links).
#[test]
fn recursive_hierarchy_is_preserved_through_rollback() {
    #[derive(Component, Reflect, Default)]
    struct GrandchildEntity;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_plugins(TransformPlugin)
        .init_resource::<FrameCounter>()
        .insert_resource(synctest_session(2))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, common::input_system)
        .rollback_component_with_reflect::<ParentEntity>()
        .rollback_component_with_reflect::<ChildEntity>()
        .rollback_component_with_reflect::<GrandchildEntity>()
        .rollback_resource_with_reflect::<FrameCounter>()
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((ParentEntity, Rollback)).with_children(|p| {
                p.spawn((ChildEntity, Rollback)).with_children(|c| {
                    c.spawn((GrandchildEntity, Rollback));
                });
            });
        })
        .add_systems(GgrsSchedule, frame_counter);

    // Run enough frames to trigger multiple rollbacks.
    for _ in 0..20 {
        app.update();
    }

    // All three levels must still exist.
    assert!(
        app.world_mut()
            .query::<&ParentEntity>()
            .iter(app.world())
            .count()
            == 1,
        "Parent should still exist after rollbacks"
    );
    assert!(
        app.world_mut()
            .query::<(&ChildEntity, &ChildOf)>()
            .iter(app.world())
            .count()
            == 1,
        "Child should still exist with a ChildOf link after rollbacks"
    );
    assert!(
        app.world_mut()
            .query::<(&GrandchildEntity, &ChildOf)>()
            .iter(app.world())
            .count()
            == 1,
        "Grandchild should still exist with a ChildOf link after rollbacks"
    );
}

/// This test makes sure that the hierarchy of entities is correctly restored when rolling back.
#[test]
fn hierarchy() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_plugins(TransformPlugin)
        .add_message::<DeleteChildEntityMessage>()
        .init_resource::<FrameCounter>()
        .add_systems(Startup, setup_system)
        .insert_resource(synctest_session(2))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, input_system)
        .rollback_component_with_reflect::<ChildEntity>()
        .rollback_component_with_reflect::<ParentEntity>()
        .rollback_resource_with_reflect::<FrameCounter>()
        .add_systems(GgrsSchedule, (frame_counter, delete_child_system).chain());

    // Re-usable queries
    let get_queries = |app: &mut App| {
        (
            app.world_mut().query::<(&ChildEntity, &ChildOf)>(),
            app.world_mut().query::<&ParentEntity>(),
        )
    };

    // Update once, the world should now be setup
    app.update();
    let (mut child_query, mut parent_query) = get_queries(&mut app);
    assert!(
        child_query.single(app.world()).is_ok(),
        "Child doesn't exist"
    );
    assert!(
        parent_query.single(app.world()).is_ok(),
        "Parent doesn't exist"
    );

    app.update();

    // Send the message to delete the child entity
    app.world_mut()
        .resource_mut::<Messages<DeleteChildEntityMessage>>()
        .write(DeleteChildEntityMessage);

    // Run for a number of times to make sure we get some rollbacks to happen
    for _ in 0..5 {
        app.update();
    }

    // Make sure the child is deleted and the parent still exists
    let (mut child_query, mut parent_query) = get_queries(&mut app);
    assert!(
        child_query.single(app.world()).is_err(),
        "Child exists after deletion"
    );
    assert!(
        parent_query.single(app.world()).is_ok(),
        "Parent doesn't exist"
    );
}

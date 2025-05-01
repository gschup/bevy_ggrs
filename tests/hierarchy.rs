use bevy::{platform::collections::HashMap, prelude::*};
use bevy_ggrs::*;
use core::time::Duration;
use ggrs::*;

pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = u8;
    type State = u8;
    type Address = usize;
}

#[derive(Reflect, Component, Default)]
struct ChildEntity;

#[derive(Reflect, Component, Default)]
struct ParentEntity;

#[derive(Reflect, Resource, Default, Debug)]
struct FrameCounter(u16);

fn input_system(mut commands: Commands, mut delete_events: EventReader<DeleteChildEntityEvent>) {
    let should_delete = u8::from(delete_events.read().count() > 0);

    let mut local_inputs = HashMap::new();
    local_inputs.insert(0, should_delete);

    commands.insert_resource(LocalInputs::<GgrsConfig>(local_inputs));
}

fn setup_system(mut commands: Commands) {
    commands
        .spawn(ParentEntity)
        .add_rollback()
        .with_children(|parent| {
            parent.spawn(ChildEntity).add_rollback();
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

#[derive(Event)]
struct DeleteChildEntityEvent;

/// This test makes sure that the hiearchy of entities is correctly restored when rolling back.
#[test]
fn hierarchy() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .add_event::<DeleteChildEntityEvent>()
        .init_resource::<FrameCounter>()
        .add_systems(Startup, setup_system)
        // Insert the GGRS session
        .insert_resource(Session::SyncTest(
            SessionBuilder::<GgrsConfig>::new()
                .with_num_players(1)
                .with_check_distance(2)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .set_rollback_schedule_fps(60)
        .add_systems(ReadInputs, input_system)
        .rollback_component_with_reflect::<ChildEntity>()
        .rollback_component_with_reflect::<ParentEntity>()
        .rollback_resource_with_reflect::<FrameCounter>()
        .add_systems(GgrsSchedule, (frame_counter, delete_child_system).chain());

    // Sleep helper that will make sure at least one frame should be executed by the GGRS fixed
    // update loop.
    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

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

    sleep();
    app.update();

    // Send the event to delete the child entity
    app.world_mut()
        .resource_mut::<Events<DeleteChildEntityEvent>>()
        .send(DeleteChildEntityEvent);

    // Run for a number of times to make sure we get some rollbacks to happen
    for _ in 0..5 {
        sleep();
        app.update();
    }

    // Make sure the child is delete and the parent still exists
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

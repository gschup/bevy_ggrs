use bevy::prelude::*;

use bevy_ggrs::*;
use ggrs::*;
use instant::Duration;

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

fn input_system(_: In<PlayerHandle>, mut delete_events: EventReader<DeleteChildEntityEvent>) -> u8 {
    u8::from(delete_events.iter().count() > 0)
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

    if let Ok(child) = child.get_single() {
        println!("Child exists: {child:?}");
    }

    if inputs[0].0 == 1 {
        println!("Despawning child");
        let child_entity = parent.single()[0];
        commands.entity(child_entity).despawn();
    }
}

fn frame_counter(mut counter: ResMut<FrameCounter>) {
    println!("==== Frame {} ====", counter.0);
    counter.0 = counter.0.wrapping_add(1);
}

#[derive(Event)]
struct DeleteChildEntityEvent;

/// This test makes sure that we correctly map entities stored in resource and components during
/// snapshot and restore.
#[test]
fn entity_mapping() {
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
        .add_ggrs_plugin(
            GgrsPlugin::<GgrsConfig>::new()
                .with_update_frequency(60)
                .with_input_system(input_system)
                .register_rollback_component::<ChildEntity>()
                .register_rollback_component::<ParentEntity>()
                .register_rollback_resource::<FrameCounter>(),
        )
        .add_systems(GgrsSchedule, (frame_counter, delete_child_system).chain());

    // Sleep helper that will make sure at least one frame should be executed by the GGRS fixed
    // update loop.
    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

    // Re-usable queries
    let get_queries = |app: &mut App| {
        (
            app.world.query::<(&ChildEntity, &Parent)>(),
            app.world.query::<(&ParentEntity, &Children)>(),
        )
    };

    // Update once, the world should now be setup
    app.update();
    let (mut child_query, mut parent_query) = get_queries(&mut app);
    assert!(
        child_query.get_single(&app.world).is_ok(),
        "Child doesn't exist"
    );
    assert!(
        parent_query.get_single(&app.world).is_ok(),
        "Parent doesn't exist"
    );

    sleep();
    app.update();

    // Send the event to delete the child entity
    app.world
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
        child_query.get_single(&app.world).is_err(),
        "Child exists after deletion"
    );
    assert!(
        parent_query.get_single(&app.world).is_ok(),
        "Parent doesn't exist"
    );
}

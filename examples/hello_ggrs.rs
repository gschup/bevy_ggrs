use bevy::{core::FixedTimestep, prelude::*};
use bevy_ggrs::{GGRSAppBuilder, GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use ggrs::PlayerHandle;

const NUM_PLAYERS: u32 = 2;
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const CHECK_DISTANCE: u32 = 7;

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;

fn main() {
    let sync_sess = ggrs::start_synctest_session(NUM_PLAYERS, INPUT_SIZE, CHECK_DISTANCE).unwrap();

    App::build()
        .add_plugins(DefaultPlugins)
        .add_plugin(GGRSPlugin)
        // define session type
        .with_session_type(SessionType::SyncTestSession)
        // define frequency of game logic update
        .with_rollback_run_criteria(FixedTimestep::steps_per_second(60.0))
        // define system that creates a compact input representation
        .with_input_system(input.system())
        // register components that will be loaded/saved
        .register_rollback_type::<Position>()
        // insert the GGRS session and rollback ID provider
        .insert_resource(sync_sess)
        .insert_resource(RollbackIdProvider::default())
        // these systems will be executed as part of the advance frame update
        .add_rollback_system(move_persons.system())
        // spawn some test entities
        .add_startup_system(spawn_persons.system())
        .run();
}

fn spawn_persons(mut rip: ResMut<RollbackIdProvider>, mut commands: Commands) {
    commands
        .spawn()
        .insert(Person)
        .insert(Name("Bernd Brems".to_string()))
        .insert(Position { x: 0, y: 0, z: 0 })
        .insert(Rollback::new(rip.next_id()));

    commands
        .spawn()
        .insert(Person)
        .insert(Name("Olga Orstrom".to_string()))
        .insert(Position { x: 0, y: 0, z: 0 })
        .insert(Rollback::new(rip.next_id()));

    commands
        .spawn()
        .insert(Person)
        .insert(Name("Hans Röst".to_string()))
        .insert(Position { x: 0, y: 0, z: 0 })
        .insert(Rollback::new(rip.next_id()));
}

fn move_persons(mut query: Query<(&Person, &mut Position), With<Rollback>>) {
    for (_, mut pos) in query.iter_mut() {
        pos.x += 1;
        pos.y += 1;
        pos.z += 1;
    }
}

/*
fn print_persons(query: Query<(&Person, &Name, &Position), With<Rollback>>) {
    for (_, name, pos) in query.iter() {
        println!("PERSON {} AT POS: {:?}", name.0, pos);
    }
}
*/

fn input(_handle: In<PlayerHandle>, keyboard_input: Res<Input<KeyCode>>) -> Vec<u8> {
    let mut input: u8 = 0;

    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    if keyboard_input.pressed(KeyCode::A) {
        input |= INPUT_LEFT;
    }
    if keyboard_input.pressed(KeyCode::S) {
        input |= INPUT_DOWN;
    }
    if keyboard_input.pressed(KeyCode::D) {
        input |= INPUT_RIGHT;
    }

    vec![input]
}

#[derive(Default, Debug, Reflect)]
#[reflect(Component)]
struct Person;

#[derive(Default, Debug, Reflect)]
#[reflect(Component)]
struct Name(String);

#[derive(Default, Debug, Reflect)]
#[reflect(Component)]
struct Position {
    x: i32,
    y: i32,
    z: i32,
}

use bevy::{core::FixedTimestep, prelude::*};
use bevy_ggrs::{GGRSAppBuilder, GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use ggrs::PlayerHandle;

const NUM_PLAYERS: u32 = 2;
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const CHECK_DISTANCE: u32 = 7;

fn main() {
    // start a GGRS SyncTest session, which will simulate rollbacks every frame

    // WARNING: usually, SyncTestSession does compare checksums to validate game update determinism,
    // but bevy_ggrs currently computes no checksums for gamestates
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

// Every entity that you want to be saved/loaded needs a `Rollback` component with a unique rollback id.
// When loading entities from the past, this extra id is necessary to connect entities over different game states
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

// Example system that mutates some variables, added as a rollback system above.
// Filtering for the rollback component is a good way to make sure your game logic systems
// only mutate components that are being saved/loaded.
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

// This system should represent player input as a `Vec<u8>`, but this example is independent from any inputs.
// Check the other examples on ideas how that could be done
fn input(_handle: In<PlayerHandle>) -> Vec<u8> {
    vec![0u8]
}

#[derive(Default, Debug)]
struct Person;

#[derive(Default, Debug)]
struct Name(String);

// Components that should be saved/loaded need to implement the `Reflect` trait
#[derive(Default, Debug, Reflect)]
#[reflect(Component)]
struct Position {
    x: i32,
    y: i32,
    z: i32,
}

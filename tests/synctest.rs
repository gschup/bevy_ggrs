use bevy::{platform::collections::HashMap, prelude::*};
use bevy_ggrs::{prelude::*, *};
use core::time::Duration;
use ggrs::*;

pub struct GgrsConfig;
impl Config for GgrsConfig {
    type Input = u8;
    type State = u8;
    type Address = usize;
}

#[derive(Reflect, Resource, Default, Debug, Clone)]
struct FrameCounter(u16);

fn frame_counter(mut counter: ResMut<FrameCounter>) {
    counter.0 = counter.0.wrapping_add(1);
}

fn input_system(mut commands: Commands, players: Res<LocalPlayers>) {
    let mut inputs = HashMap::new();
    for &handle in &players.0 {
        inputs.insert(handle, 0u8);
    }
    commands.insert_resource(LocalInputs::<GgrsConfig>(inputs));
}

fn create_app(check_distance: usize) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<FrameCounter>()
        .insert_resource(Session::SyncTest(
            SessionBuilder::<GgrsConfig>::new()
                .with_num_players(1)
                .unwrap()
                .with_check_distance(check_distance)
                .add_player(PlayerType::Local, 0)
                .unwrap()
                .start_synctest_session()
                .unwrap(),
        ))
        .add_plugins(GgrsPlugin::<GgrsConfig>::default())
        .add_systems(ReadInputs, input_system)
        .rollback_resource_with_clone::<FrameCounter>()
        .add_systems(GgrsSchedule, frame_counter);
    app
}

/// Verifies that `ConfirmedFrameCount` advances for SyncTest sessions and that old snapshots are
/// pruned once confirmed. Regression test for the inverted confirmed frame condition bug.
#[test]
fn synctest_prunes_confirmed_snapshots() {
    let check_distance: usize = 5;
    let mut app = create_app(check_distance);
    let sleep = || std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));

    // Run enough frames for ConfirmedFrameCount to advance well past 0
    for _ in 0..20 {
        sleep();
        app.update();
    }

    let confirmed = app.world().resource::<ConfirmedFrameCount>().0;
    assert!(
        confirmed > 0,
        "ConfirmedFrameCount should advance for SyncTest sessions, got {confirmed}"
    );

    // Snapshots at or before the confirmed frame should have been pruned
    let snapshots = app
        .world()
        .resource::<GgrsResourceSnapshots<FrameCounter>>();
    assert!(
        snapshots.peek(0).is_none(),
        "Frame 0 snapshot should have been pruned (confirmed_frame={confirmed})"
    );
}

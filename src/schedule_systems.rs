use crate::{
    world_snapshot::{RollbackSnapshots, WorldSnapshot},
    FixedTimestepData, GgrsSchedule, LoadWorld, LocalInputs, LocalPlayers, PlayerInputs,
    ReadInputs, RollbackFrameCount, RollbackTypeRegistry, SaveWorld, Session,
};
use bevy::{prelude::*, utils::Duration};
use ggrs::{
    Config, GGRSError, GGRSRequest, InputStatus, P2PSession, SessionState, SpectatorSession,
    SyncTestSession,
};

pub(crate) fn run_ggrs_schedules<T: Config>(world: &mut World) {
    let mut time_data = world
        .remove_resource::<FixedTimestepData>()
        .expect("failed to extract GGRS FixedTimeStepData");

    let delta = world
        .get_resource::<Time>()
        .expect("Time resource not found, did you remove it?")
        .delta();

    let mut fps_delta = 1. / time_data.fps as f64;
    if time_data.run_slow {
        fps_delta *= 1.1;
    }
    time_data.accumulator = time_data.accumulator.saturating_add(delta);

    // no matter what, poll remotes and send responses
    if let Some(mut session) = world.get_resource_mut::<Session<T>>() {
        match &mut *session {
            Session::P2P(session) => {
                session.poll_remote_clients();
            }
            Session::Spectator(session) => {
                session.poll_remote_clients();
            }
            _ => {}
        }
    }

    // if we accumulated enough time, do steps
    while time_data.accumulator.as_secs_f64() > fps_delta {
        // decrease accumulator
        time_data.accumulator = time_data
            .accumulator
            .saturating_sub(Duration::from_secs_f64(fps_delta));

        // depending on the session type, doing a single update looks a bit different
        let session = world.remove_resource::<Session<T>>();
        match session {
            Some(Session::SyncTest(s)) => run_synctest::<T>(world, s),
            Some(Session::P2P(session)) => {
                // if we are ahead, run slow
                time_data.run_slow = session.frames_ahead() > 0;

                run_p2p(world, session);
            }
            Some(Session::Spectator(s)) => run_spectator(world, s),
            _ => {
                // No session has been started yet, reset time data and snapshots
                time_data.accumulator = Duration::ZERO;
                time_data.run_slow = false;
                world.insert_resource(LocalPlayers::default());
                world.insert_resource(RollbackSnapshots::default());
                world.insert_resource(RollbackFrameCount(0));
            }
        }
    }

    world.insert_resource(time_data);
}

pub(crate) fn run_synctest<C: Config>(world: &mut World, mut sess: SyncTestSession<C>) {
    maybe_init_snapshots(world, sess.max_prediction());

    world.insert_resource(LocalPlayers((0..sess.num_players()).collect()));

    // read local player inputs and register them in the session
    world.run_schedule(ReadInputs);
    let local_inputs = world.remove_resource::<LocalInputs<C>>().expect(
        "No local player inputs found. Did you insert systems into the ReadInputs schedule?",
    );
    for (handle, input) in local_inputs.0 {
        sess.add_local_input(handle, input)
            .expect("All handles in local_handles should be valid");
    }

    match sess.advance_frame() {
        Ok(requests) => handle_requests(requests, world),
        Err(e) => warn!("{e}"),
    }

    world.insert_resource(Session::SyncTest(sess));
}

pub(crate) fn run_spectator<T: Config>(world: &mut World, mut sess: SpectatorSession<T>) {
    // if session is ready, try to advance the frame
    if sess.current_state() == SessionState::Running {
        match sess.advance_frame() {
            Ok(requests) => handle_requests(requests, world),
            Err(GGRSError::PredictionThreshold) => {
                info!("P2PSpectatorSession: Waiting for input from host.")
            }
            Err(e) => warn!("{e}"),
        };
    }

    world.insert_resource(Session::Spectator(sess));
}

pub(crate) fn run_p2p<C: Config>(world: &mut World, mut sess: P2PSession<C>) {
    maybe_init_snapshots(world, sess.max_prediction());

    world.insert_resource(LocalPlayers(sess.local_player_handles()));

    if sess.current_state() == SessionState::Running {
        // get local player inputs
        world.run_schedule(ReadInputs);
        let local_inputs = world.remove_resource::<LocalInputs<C>>().expect(
            "No local player inputs found. Did you insert systems into the ReadInputs schedule?",
        );
        for (handle, input) in local_inputs.0 {
            sess.add_local_input(handle, input)
                .expect("All handles in local_inputs should be valid");
        }

        match sess.advance_frame() {
            Ok(requests) => handle_requests(requests, world),
            Err(GGRSError::PredictionThreshold) => {
                info!("Skipping a frame: PredictionThreshold.")
            }
            Err(e) => warn!("{e}"),
        };
    }

    world.insert_resource(Session::P2P(sess));
}

pub(crate) fn handle_requests<T: Config>(requests: Vec<GGRSRequest<T>>, world: &mut World) {
    for request in requests {
        match request {
            GGRSRequest::SaveGameState { cell, frame } => {
                debug!("saving snapshot for frame {frame}");
                world.run_schedule(SaveWorld);

                // look into resources and find the checksum
                let snapshots = world
                    .get_resource::<RollbackSnapshots>()
                    .expect("No GGRS RollbackSnapshots resource found. Did you remove it?");

                // todo: make accessor?
                let pos = frame as usize % snapshots.0.len();
                let checksum = snapshots.0[pos].checksum;

                // we don't really use the buffer provided by GGRS
                cell.save(frame, None, Some(checksum as u128));
            }
            GGRSRequest::LoadGameState { frame, .. } => {
                // we don't really use the buffer provided by GGRS
                debug!("restoring snapshot for frame {frame}");

                world
                    .get_resource_mut::<RollbackFrameCount>()
                    .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
                    .0 = frame;

                world.run_schedule(LoadWorld);
            }
            GGRSRequest::AdvanceFrame { inputs } => advance_frame::<T>(inputs, world),
        }
    }
}

pub(crate) fn save_world(world: &mut World) {
    // we make a snapshot of our world
    let rollback_registry = world
        .remove_resource::<RollbackTypeRegistry>()
        .expect("GGRS type registry not found. Did you remove it?");
    let snapshot = WorldSnapshot::from_world(world, &rollback_registry.0);
    world.insert_resource(rollback_registry);

    let frame = world
        .get_resource::<RollbackFrameCount>()
        .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?")
        .0;

    let mut snapshots = world
        .get_resource_mut::<RollbackSnapshots>()
        .expect("No GGRS RollbackSnapshots resource found. Did you remove it?");

    // store the snapshot ourselves (since the snapshots don't implement clone)
    let pos = frame as usize % snapshots.0.len();
    snapshots.0[pos] = snapshot;
}

pub(crate) fn load_world(world: &mut World) {
    let frame = world
        .get_resource::<RollbackFrameCount>()
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

pub(crate) fn advance_frame<T: Config>(inputs: Vec<(T::Input, InputStatus)>, world: &mut World) {
    let mut frame_count = world
        .get_resource_mut::<RollbackFrameCount>()
        .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?");

    frame_count.0 += 1;
    let frame = frame_count.0;

    debug!("advancing to frame: {}", frame);
    world.insert_resource(PlayerInputs::<T>(inputs));
    world.run_schedule(GgrsSchedule);
    world.remove_resource::<PlayerInputs<T>>();
    debug!("frame {frame} completed");
}

fn maybe_init_snapshots(world: &mut World, len: usize) {
    let snapshots = &mut world
        .get_resource_mut::<RollbackSnapshots>()
        .expect("No GGRS RollbackSnapshots resource found. Did you remove it?")
        .0;

    if snapshots.len() != len {
        snapshots.clear();
        for _ in 0..len {
            snapshots.push(WorldSnapshot::default());
        }
    }
}

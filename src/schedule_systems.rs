use bevy::prelude::*;
use ggrs::{
    Config, GGRSError, GGRSRequest, P2PSession, SessionState, SpectatorSession, SyncTestSession,
};
use instant::{Duration, Instant};

use crate::{
    world_snapshot::{RollbackSnapshots, WorldSnapshot},
    AdvanceFrame, FixedTimestepData, LoadWorld, LocalInputs, ReadInputs, RollbackFrameCount,
    SaveWorld, Session, SynchronizedInputs,
};

pub fn run_ggrs_schedules<C: Config>(world: &mut World) {
    let mut time_data = world
        .remove_resource::<FixedTimestepData>()
        .expect("Unable to find GGRS FixedTimestepData. Did you remove it?");
    // no matter what, poll remotes and send responses
    if let Some(mut session) = world.get_resource_mut::<Session<C>>() {
        match &mut *session {
            Session::P2PSession(session) => {
                session.poll_remote_clients();
                time_data.run_slow = session.frames_ahead() > 0;
            }
            Session::SpectatorSession(session) => {
                session.poll_remote_clients();
            }
            _ => {}
        }
    }

    // get delta time from last call and accumulate it
    let delta = Instant::now().duration_since(time_data.last_update);
    let mut fps_delta = 1. / time_data.fps as f64;
    if time_data.run_slow {
        fps_delta *= 1.1;
    }
    time_data.accumulator = time_data.accumulator.saturating_add(delta);
    time_data.last_update = Instant::now();

    // if we accumulated enough time, do steps
    while time_data.accumulator.as_secs_f64() > fps_delta {
        // decrease accumulator
        time_data.accumulator = time_data
            .accumulator
            .saturating_sub(Duration::from_secs_f64(fps_delta));

        // depending on the session type, doing a single update looks a bit different
        // session gets reinserted afterwards
        let session = world.remove_resource::<Session<C>>();
        match session {
            Some(Session::SyncTestSession(sess)) => run_synctest(world, sess),
            Some(Session::P2PSession(sess)) => run_p2p(world, sess),
            Some(Session::SpectatorSession(sess)) => run_spectator(world, sess),
            _ => {
                // No session has been started yet, reset time data and snapshots
                time_data.last_update = Instant::now();
                time_data.accumulator = Duration::ZERO;
                time_data.run_slow = false;
                world.insert_resource(RollbackSnapshots::default());
                world.insert_resource(RollbackFrameCount(0));
            }
        }
    }

    world.insert_resource(time_data);
}

pub fn run_p2p<C: Config>(world: &mut World, mut sess: P2PSession<C>) {
    // maybe init snapshots
    init_snapshots(world, sess.max_prediction());

    // if session is ready, try to advance the frame
    if sess.current_state() == SessionState::Running {
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
            Err(GGRSError::PredictionThreshold) => {
                info!("Skipping a frame: PredictionThreshold.")
            }
            Err(e) => println!("{}", e),
        };
    }

    // re-insert session into world
    world.insert_resource(Session::P2PSession(sess));
}

pub fn run_spectator<C: Config>(world: &mut World, mut sess: SpectatorSession<C>) {
    // if session is ready, try to advance the frame
    if sess.current_state() == SessionState::Running {
        match sess.advance_frame() {
            Ok(requests) => handle_requests(requests, world),
            Err(GGRSError::PredictionThreshold) => {
                info!("P2PSpectatorSession: Waiting for input from host.")
            }
            Err(e) => warn!("{}", e),
        };
    }

    // re-insert session into world
    world.insert_resource(Session::SpectatorSession(sess));
}

pub fn run_synctest<C: Config>(world: &mut World, mut sess: SyncTestSession<C>) {
    // maybe init snapshots
    init_snapshots(world, sess.max_prediction());

    // read local player inputs and register them in the session
    world.run_schedule(ReadInputs);
    let local_inputs = world.remove_resource::<LocalInputs<C>>().expect(
        "No local player inputs found. Did you insert systems into the ReadInputs schedule?",
    );
    for (handle, input) in local_inputs.0 {
        sess.add_local_input(handle, input)
            .expect("All handles in local_handles should be valid");
    }
    // try to advance the frame
    match sess.advance_frame() {
        Ok(requests) => handle_requests(requests, world),
        Err(e) => println!("{}", e),
    };

    // re-insert session into world
    world.insert_resource(Session::SyncTestSession(sess));
}

pub fn handle_requests<C: Config>(requests: Vec<GGRSRequest<C>>, world: &mut World) {
    for request in requests {
        match request {
            GGRSRequest::SaveGameState { cell, frame } => {
                debug!("saving snapshot for frame {frame}");
                world.run_schedule(SaveWorld);
                // we don't really use the buffer provided by GGRS
                // TODO: CHECKSUM
                cell.save(frame, None, None);
            }
            GGRSRequest::LoadGameState { frame, .. } => {
                debug!("restoring snapshot for frame {frame}");
                world.run_schedule(LoadWorld);
            }
            GGRSRequest::AdvanceFrame { inputs } => {
                let mut frame_count = world
                    .get_resource_mut::<RollbackFrameCount>()
                    .expect("Unable to find GGRS RollbackFrameCount. Did you remove it?");
                frame_count.0 += 1;
                let frame = frame_count.0;
                debug!("advancing to frame: {}", frame);
                world.insert_resource(SynchronizedInputs::<C>(inputs));
                world.run_schedule(AdvanceFrame);
                world.remove_resource::<SynchronizedInputs<C>>();
                debug!("frame {} completed", frame);
            }
        }
    }
}

pub fn init_snapshots(world: &mut World, len: usize) {
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

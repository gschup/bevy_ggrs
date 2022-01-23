mod stubs;

use crate::stubs::FakeSocket;
use bevy::prelude::*;
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use ggrs::P2PSession;
use stubs::*;

#[test]
fn test_create_generic_addr_session() {
    let fake_socket = FakeSocket::new();
    let _sess = P2PSession::<Vec<u8>, String>::new_with_socket(
        2,
        stubs::INPUT_SIZE,
        stubs::MAX_PRED_FRAMES,
        fake_socket,
    );
}

#[test]
fn test_create_generic_addr_bevy_app() {
    let fake_socket = FakeSocket::new();
    let mut sess = P2PSession::<Vec<u8>, String>::new_with_socket(
        2,
        stubs::INPUT_SIZE,
        stubs::MAX_PRED_FRAMES,
        fake_socket,
    );

    // set default expected update frequency (affects synchronization timings between players)
    sess.set_fps(60).unwrap();

    App::new().add_plugin(GGRSPlugin).with_p2p_session(sess);
}

#[test]
fn test_start_sess_generic_addr_bevy_app() {
    let fake_socket = FakeSocket::new();
    let mut sess = P2PSession::<Vec<u8>, String>::new_with_socket(
        2,
        stubs::INPUT_SIZE,
        stubs::MAX_PRED_FRAMES,
        fake_socket,
    );

    sess.set_fps(60).unwrap();

    App::new()
        .add_plugin(GGRSPlugin)
        .with_p2p_session(sess)
        .add_startup_system(start_p2p_session);
}

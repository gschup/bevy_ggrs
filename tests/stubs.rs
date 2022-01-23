use bevy::prelude::*;
use ggrs::{NonBlockingSocket, P2PSession, PlayerType};

pub const INPUT_SIZE: usize = std::mem::size_of::<u32>();
pub const MAX_PRED_FRAMES: usize = 8;

pub struct FakeSocket;

impl FakeSocket {
    pub fn new() -> Self {
        Self {}
    }
}

impl NonBlockingSocket<String> for FakeSocket {
    fn send_to(&mut self, _msg: &ggrs::UdpMessage, _addr: &String) {}

    fn receive_all_messages(&mut self) -> Vec<(String, ggrs::UdpMessage)> {
        vec![]
    }
}

// systems
pub fn start_p2p_session(mut p2p_sess: ResMut<P2PSession<Vec<u8>, String>>) {
    p2p_sess.add_player(PlayerType::Local, 0).unwrap();
    let remote_addr = "dummy_addr".to_owned();
    p2p_sess
        .add_player(PlayerType::Remote(remote_addr), 1)
        .unwrap();
    let spec_addr = "dummy_addr".to_owned();
    p2p_sess
        .add_player(PlayerType::Spectator(spec_addr), 2)
        .unwrap();
    p2p_sess.start_session().unwrap();
}

mod stubs;

use crate::stubs::{FakeSocket, StubConfig};
use bevy::prelude::*;
use bevy_ggrs::{GGRSApp, GGRSPlugin};
use ggrs::{GGRSError, PlayerType, SessionBuilder};

#[test]
fn test_create_generic_addr_bevy_app() -> Result<(), GGRSError> {
    let fake_socket = FakeSocket::new();
    let sess = SessionBuilder::<StubConfig>::new()
        .with_num_players(2)
        .add_player(PlayerType::Local, 0)?
        .add_player(PlayerType::Remote("fake_addr".to_owned()), 1)?
        .start_p2p_session(fake_socket)?;

    App::new()
        .add_plugin(GGRSPlugin::<StubConfig>::default())
        .with_p2p_session(sess);

    Ok(())
}

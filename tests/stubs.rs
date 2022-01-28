use bytemuck::{Pod, Zeroable};
use ggrs::{Config, NonBlockingSocket};

pub struct StubConfig;

impl Config for StubConfig {
    type Input = StubInput;
    type State = u8;
    type Address = String;
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Pod, Zeroable)]
pub struct StubInput {
    pub inp: u32,
}

pub struct FakeSocket;

impl FakeSocket {
    pub fn new() -> Self {
        Self {}
    }
}

impl NonBlockingSocket<String> for FakeSocket {
    fn send_to(&mut self, _msg: &ggrs::Message, _addr: &String) {}

    fn receive_all_messages(&mut self) -> Vec<(String, ggrs::Message)> {
        vec![]
    }
}

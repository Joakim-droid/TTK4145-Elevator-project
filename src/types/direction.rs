use driver_rust::elevio::elev::{DIRN_DOWN, DIRN_STOP, DIRN_UP};
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    #[serde(rename = "up")]
    Up = DIRN_UP,
    #[serde(rename = "down")]
    Down = DIRN_DOWN,
    #[serde(rename = "stop")]
    Stop = DIRN_STOP,
}

impl From<Direction> for u8 {
    fn from(d: Direction) -> Self {
        d as u8
    }
}
use driver_rust::elevio::elev::{DIRN_DOWN, DIRN_STOP, DIRN_UP, Elevator};
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up = DIRN_UP,
    Down = DIRN_DOWN,
    Stop = DIRN_STOP,
}

impl From<Direction> for u8 {
    fn from(d: Direction) -> Self {
        d as u8
    }
}

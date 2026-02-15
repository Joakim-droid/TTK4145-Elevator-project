use std::time::SystemTime;

use crate::config::NUM_FLOORS;
use driver_rust::elevio::elev::DIRN_DOWN;
use driver_rust::elevio::elev::DIRN_STOP;
use driver_rust::elevio::elev::DIRN_UP;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum Behaviour {
    #[default]
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "moving")]
    Moving,
    #[serde(rename = "doorOpen")]
    DoorOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    #[serde(rename = "up")]
    Up,
    #[serde(rename = "down")]
    Down,
    #[serde(rename = "stop")]
    Stop,
}

impl Direction {
    pub fn to_u8(&self) -> u8 {
        match *self {
            Direction::Up => DIRN_UP,
            Direction::Down => DIRN_DOWN,
            Direction::Stop => DIRN_STOP,
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElevatorState {
    pub floor: Option<u8>,
    pub behaviour: Behaviour,
    pub direction: u8,
    #[serde(rename = "cabRequests")]
    pub cab_requests: [bool; NUM_FLOORS],
    pub door_open: bool,

    // Skip the doortimer when serializing the struct for broadcasting
    #[serde(skip)]
    #[serde(default)]
    pub door_timer: Option<SystemTime>,
}

impl Default for ElevatorState {
    fn default() -> Self {
        Self {
            floor: None,
            behaviour: Behaviour::Idle,
            direction: DIRN_STOP,
            cab_requests: [false; NUM_FLOORS],
            door_timer: None,
            door_open: false,
        }
    }
}

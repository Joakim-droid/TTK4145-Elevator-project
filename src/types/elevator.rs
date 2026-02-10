use std::time::SystemTime;

use crate::config::NUM_FLOORS;
use driver_rust::elevio::elev::DIRN_STOP;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum Behaviour {
    #[default]
    Idle,
    Moving,
    DoorOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElevatorState {
    pub floor: Option<u8>,
    pub behaviour: Behaviour,
    pub direction: u8,
    pub cab_requests: [bool; NUM_FLOORS],
    pub door_open: bool,
    pub obstruction: bool,

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
            obstruction: false,
        }
    }
}

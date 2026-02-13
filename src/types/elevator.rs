use std::time::SystemTime;

use crate::config::NUM_FLOORS;
use driver_rust::elevio::elev::{DIRN_DOWN, DIRN_STOP, DIRN_UP};
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
    floor: Option<u8>,
    behaviour: Behaviour,
    direction: u8,
    cab_requests: [bool; NUM_FLOORS],
    door_open: bool,

    // Skip the doortimer when serializing the struct for broadcasting
    #[serde(skip)]
    #[serde(default)]
    door_timer: Option<SystemTime>,
}

impl ElevatorState {
    pub fn stop(&mut self) {
        self.behaviour = Behaviour::Idle;
        self.direction = DIRN_STOP;
    }

    pub fn get_behavior(&self) -> Behaviour {
        self.behaviour
    }

    pub fn set_floor(&mut self, floor: u8) {
        self.floor = Some(floor);
    }

    pub fn get_floor(&self) -> Option<u8> {
        self.floor
    }

    pub fn set_cab_request(&mut self, floor: u8) {
        self.cab_requests[floor as usize] = true;
    }

    pub fn open_door(&mut self) {
        if self.floor.is_none() {
            eprintln!("Cant open door in between floors");
            return;
        }

        if self.behaviour == Behaviour::Moving {
            self.stop();
        }
        self.door_timer = Some(SystemTime::now());
        self.behaviour = Behaviour::DoorOpen;
    }

    pub fn close_door(&mut self) {
        if self.behaviour != Behaviour::DoorOpen {
            eprintln!("Should not call close door if door is not open");
            return;
        }

        self.door_timer = None;
        self.behaviour = Behaviour::Idle;
    }

    pub fn set_direction(&mut self, direction: u8) {
        if direction == DIRN_STOP {
            self.stop();
        } else if direction == DIRN_UP {
            self.direction = DIRN_UP;
            self.behaviour = Behaviour::Moving
        } else {
            self.direction = DIRN_DOWN;
            self.behaviour = Behaviour::Moving;
        }
    }

    pub fn get_door_timer(&self) -> Option<SystemTime> {
        self.door_timer
    }
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

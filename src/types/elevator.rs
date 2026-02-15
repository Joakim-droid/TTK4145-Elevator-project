use crate::{config::NUM_FLOORS, types::direction::Direction};
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
    direction: Direction,
    cab_requests: [bool; NUM_FLOORS],
    door_open: bool,
    obstruction: bool,
    emergency_stop: bool,
    // Keeps track of the active door timer
    door_open_counter: u64,
}

impl ElevatorState {
    pub fn stop(&mut self) {
        self.behaviour = Behaviour::Idle;
        self.direction = Direction::Stop;
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

    pub fn add_cab_request(&mut self, floor: u8) {
        if floor >= NUM_FLOORS as u8 {
            println!("Invalid floor requested");
            return;
        }

        self.cab_requests[floor as usize] = true;
    }

    pub fn clear_cab_request(&mut self, floor: u8) {
        if floor >= NUM_FLOORS as u8 {
            println!("Invalid floor cleared");
            return;
        }
        self.cab_requests[floor as usize] = false;
    }

    pub fn open_door(&mut self) -> Option<u64> {
        if self.floor.is_none() {
            eprintln!("Cant open door in between floors");
            return None;
        }

        if self.behaviour == Behaviour::Moving {
            self.stop();
        }

        self.behaviour = Behaviour::DoorOpen;

        self.door_open_counter += 1;
        Some(self.door_open_counter)
    }

    pub fn close_door(&mut self) {
        if self.behaviour != Behaviour::DoorOpen {
            eprintln!("Should not call close door if door is not open");
            return;
        }

        self.behaviour = Behaviour::Idle;
    }

    pub fn set_direction(&mut self, direction: Direction) {
        if direction == Direction::Stop {
            self.stop();
        } else if direction == Direction::Up {
            self.direction = Direction::Up;
            self.behaviour = Behaviour::Moving
        } else {
            self.direction = Direction::Down;
            self.behaviour = Behaviour::Moving;
        }
    }

    pub fn get_direction(&self) -> Direction {
        self.direction
    }

    pub fn get_current_timer_id(&self) -> u64 {
        self.door_open_counter
    }

    pub fn is_obstructed(&self) -> bool {
        self.obstruction
    }

    pub fn set_obstructed(&mut self, is_obstructed: bool) {
        self.obstruction = is_obstructed;
    }

    pub fn is_emergency_stop(&self) -> bool {
        self.emergency_stop
    }

    pub fn set_emergency_stop(&mut self, is_pressed: bool) {
        self.emergency_stop = is_pressed
    }
}

impl Default for ElevatorState {
    fn default() -> Self {
        Self {
            floor: None,
            behaviour: Behaviour::Idle,
            direction: Direction::Stop,
            cab_requests: [false; NUM_FLOORS],
            door_open: false,
            door_open_counter: 0,
            obstruction: false,
            emergency_stop: false,
        }
    }
}

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
    obstruction: bool,
    door_open_counter: u64,
    seq: u64,
}

impl ElevatorState {
    fn bump_seq(&mut self) {
        self.seq = self.seq.saturating_add(1);
    }

    pub fn stop(&mut self) {
        self.behaviour = Behaviour::Idle;
        self.direction = DIRN_STOP;
        self.bump_seq();
    }

    pub fn get_behavior(&self) -> Behaviour {
        self.behaviour
    }

    pub fn set_floor(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS{
            eprint!("Undefined floor ordered: {}", floor);
            return;
        }
        self.floor = Some(floor);
        self.bump_seq();
    }

    pub fn get_floor(&self) -> Option<u8> {
        self.floor
    }

    pub fn set_cab_request(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS{
            eprint!("Undefined floor ordered: {}", floor);
            return;
        }
        self.cab_requests[floor as usize] = true;
        self.bump_seq();
    }

    pub fn clear_cab_request(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS{
            eprint!("Undefined floor ordered: {}", floor);
            return;
        }
        self.cab_requests[floor as usize] = false;
        self.bump_seq();
    }

    pub fn get_cab_request(&self, floor: u8) -> bool {
        self.cab_requests[floor as usize]
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

        self.door_open_counter = self.door_open_counter.saturating_add(1); // TODO: Legg til hjelpefunksjon for ryddighet
        self.bump_seq();
        Some(self.door_open_counter)
    }

    pub fn close_door(&mut self) {
        if self.behaviour != Behaviour::DoorOpen {
            eprintln!("Should not call close door if door is not open");
            return;
        }

        self.behaviour = Behaviour::Idle;
        self.bump_seq();
    }

    pub fn set_direction(&mut self, direction: u8) {
        if direction == DIRN_STOP {
            self.stop();
        } else if direction == DIRN_UP {
            self.direction = DIRN_UP;
            self.behaviour = Behaviour::Moving;
            self.bump_seq();
        } else {
            self.direction = DIRN_DOWN;
            self.behaviour = Behaviour::Moving;
            self.bump_seq();
        }
    }

    pub fn get_current_timer_id(&self) -> u64 {
        self.door_open_counter
    }

    pub fn get_direction(&self) -> u8 {
        self.direction
    }

    pub fn is_door_open(&self) -> bool {
        self.behaviour == Behaviour::DoorOpen
    }

    pub fn get_seq(&self) -> u64 {
        self.seq
    }

    pub fn set_obstruction(&mut self, val: bool) {
        if self.obstruction != val {
            self.obstruction = val;
            self.bump_seq();
        }
    }

    pub fn get_obstruction(&self) -> bool {
        self.obstruction
    }
}

impl Default for ElevatorState {
    fn default() -> Self {
        Self {
            floor: None,
            behaviour: Behaviour::Idle,
            direction: DIRN_STOP,
            cab_requests: [false; NUM_FLOORS],
            door_open_counter: 0,
            obstruction: false,
            seq: 0,
        }
    }
}

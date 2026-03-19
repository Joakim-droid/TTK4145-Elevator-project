use crate::{config::NUM_FLOORS, types::direction::Direction};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElevatorState {
    floor: Option<u8>,
    behaviour: Behaviour,
    direction: Direction,
    #[serde(rename = "cabRequests")]
    cab_requests: [bool; NUM_FLOORS],
    obstruction: bool,
    emergency_stop: bool,
    door_open_counter: u64,
    boot_id: u64,
    seq: u64,
}

fn new_boot_id() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_secs()
        .saturating_mul(1_000_000_000)
        .saturating_add(now.subsec_nanos() as u64)
        ^ (std::process::id() as u64)
}

impl ElevatorState {
    fn bump_seq(&mut self) {
        self.seq = self.seq.saturating_add(1);
    }

    pub fn stop(&mut self) {
        if self.behaviour != Behaviour::Idle || self.direction != Direction::Stop {
            self.behaviour = Behaviour::Idle;
            self.direction = Direction::Stop;
            self.bump_seq();
        }
    }

    pub fn get_behavior(&self) -> Behaviour {
        self.behaviour
    }

    pub fn set_floor(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS {
            eprint!("Undefined floor ordered: {}", floor);
            return;
        }
        if self.floor != Some(floor) {
            self.floor = Some(floor);
            self.bump_seq();
        }

        if (floor as usize) >= NUM_FLOORS - 1 || (floor as usize) <= 0 {
            eprintln!("Invalid floor: {}", floor);
            self.stop();
        }
    }

    pub fn get_floor(&self) -> Option<u8> {
        self.floor
    }

    pub fn add_cab_request(&mut self, floor: u8) {
        if (floor as usize) < NUM_FLOORS && !self.cab_requests[floor as usize] {
            self.cab_requests[floor as usize] = true;
            self.bump_seq();
        }
    }

    pub fn clear_cab_request(&mut self, floor: u8) {
        if (floor as usize) < NUM_FLOORS && self.cab_requests[floor as usize] {
            self.cab_requests[floor as usize] = false;
            self.bump_seq();
        }
    }

    pub fn get_cab_request(&self, floor: u8) -> bool {
        self.cab_requests[floor as usize]
    }
    pub fn open_door(&mut self) -> Option<u64> {
        if self.floor.is_none() {
            eprintln!("Cant open door in between floors");
            return None;
        }

        self.behaviour = Behaviour::DoorOpen;

        self.door_open_counter = self.door_open_counter.saturating_add(1); // TODO: Legg til hjelpefunksjon for ryddighet
        self.bump_seq();
        Some(self.door_open_counter)
    }

    pub fn close_door(&mut self) {
        if self.behaviour == Behaviour::DoorOpen {
            self.behaviour = Behaviour::Idle;
            self.direction = Direction::Stop;
            self.bump_seq();
        }
    }

    pub fn set_direction(&mut self, direction: Direction) {
        if direction == Direction::Stop {
            self.stop();
        } else if direction == Direction::Up {
            if self.direction != Direction::Up || self.behaviour != Behaviour::Moving {
                self.direction = Direction::Up;
                self.behaviour = Behaviour::Moving;
                self.bump_seq();
            }
        } else {
            if self.direction != Direction::Down || self.behaviour != Behaviour::Moving {
                self.direction = Direction::Down;
                self.behaviour = Behaviour::Moving;
                self.bump_seq();
            }
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
    pub fn is_emergency_stop(&self) -> bool {
        self.emergency_stop
    }

    pub fn set_emergency_stop(&mut self, is_pressed: bool) {
        if self.emergency_stop != is_pressed {
            self.emergency_stop = is_pressed;
            self.bump_seq();
        }
    }

    pub fn is_newer_than(&self, other: &ElevatorState) -> bool {
        self.boot_id > other.boot_id || (self.boot_id == other.boot_id && self.seq > other.seq)
    }

    pub fn set_obstruction(&mut self, val: bool) {
        if self.obstruction != val {
            self.obstruction = val;
            self.bump_seq();
        }
    }

    pub fn recover_from_backup(&mut self, backup: &ElevatorState) -> bool {
        let mut changed = false;
        for f in 0..crate::config::NUM_FLOORS {
            if backup.cab_requests[f] && !self.cab_requests[f] {
                self.cab_requests[f] = true;
                changed = true;
            }
        }
        if changed {
            self.bump_seq();
        }
        changed
    }

    pub fn reset_seq(&mut self) {
        self.seq = 0;
    }
}

impl Default for ElevatorState {
    fn default() -> Self {
        Self {
            floor: None,
            behaviour: Behaviour::Idle,
            direction: Direction::Stop,
            cab_requests: [false; NUM_FLOORS],
            door_open_counter: 0,
            obstruction: false,
            emergency_stop: false,
            boot_id: new_boot_id(),
            seq: 0,
        }
    }
}

//! Distributed system state for all elevators, including hall orders, merge rules, and lamp updates.

use crate::types::direction::Direction;
use crate::types::elevator::{Behaviour, ElevatorState};
use crate::{config::NUM_FLOORS, types::orders::OrderType};
use core::fmt;
use driver_rust::elevio::elev::Elevator;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const HALLUP: usize = 0;
const HALLDOWN: usize = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemState {
    my_id: String,
    #[serde(rename = "states")]
    elevators: HashMap<String, ElevatorState>,
    #[serde(rename = "hallRequests")]
    hall_requests: [[bool; 2]; NUM_FLOORS],
    hall_epoch: [[u64; 2]; NUM_FLOORS],
    #[serde(skip)]
    dead_elevators: HashSet<String>,
    #[serde(skip)]
    sync_complete: bool,
}

impl SystemState {
    pub fn new(id: &str) -> SystemState {
        let mut elevators: HashMap<String, ElevatorState> = HashMap::new();
        elevators.insert(id.to_owned(), ElevatorState::default());

        SystemState {
            my_id: id.to_owned(),
            elevators,
            hall_requests: [[false; 2]; NUM_FLOORS],
            hall_epoch: [[0; 2]; NUM_FLOORS],
            dead_elevators: HashSet::new(),
            sync_complete: false,
        }
    }

    pub fn mark_sync_complete(&mut self) {
        self.sync_complete = true;
    }

    pub fn get_my_state(&mut self) -> &mut ElevatorState {
        self.elevators
            .entry(self.my_id.clone())
            .or_default()
    }

    pub fn get_my_id(&self) -> String {
        self.my_id.clone()
    }

    pub fn peer_lost(&mut self, id: &str) {
        if id == self.my_id {
            return;
        }
        self.dead_elevators.insert(id.to_owned());
    }

    pub fn peer_new(&mut self, id: &str) {
        if id == self.my_id {
            return;
        }
        self.dead_elevators.remove(id);
    }

    pub fn get_dead_elevators(&self) -> &HashSet<String> {
        &self.dead_elevators
    }

    pub fn remove_elevator_record(&mut self, id: &str) {
        self.elevators.remove(id);
    }

    pub fn get_elevator_ids(&self) -> Vec<String> {
        self.elevators.keys().cloned().collect()
    }

    pub fn get_elevator_state(&self, id: &str) -> Option<&ElevatorState> {
        self.elevators.get(id)
    }

    fn set_hall_order(&mut self, arrive: bool, floor: u8, order: OrderType) {
        match order {
            OrderType::HallUp => self.hall_requests[floor as usize][HALLUP] = arrive,
            OrderType::HallDown => self.hall_requests[floor as usize][HALLDOWN] = arrive,
            OrderType::Cab => {
                eprintln!("Should not be called with Cab order")
            }
        }
    }

    pub fn arrive_at_floor(&mut self, floor: u8) {
        let local_state = self.get_my_state();
        local_state.set_floor(floor);
    }

    pub fn add_order(&mut self, floor: u8, order: OrderType) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor ordered");
            return;
        }

        let local_state = self.get_my_state();
        
        if local_state.is_emergency_stop() {
            eprintln!("Cannot add order while in emergency stop");
            return;
        }

        match order {
            OrderType::Cab => {
                let local_state = self.get_my_state();
                local_state.add_cab_request(floor);
            }
            OrderType::HallUp => {
                if !self.hall_requests[floor as usize][HALLUP] {
                    self.bump_hall_epoch(floor as usize, order);
                    self.set_hall_order(true, floor, order);
                }
            }
            OrderType::HallDown => {
                if !self.hall_requests[floor as usize][HALLDOWN] {
                    self.bump_hall_epoch(floor as usize, order);
                    self.set_hall_order(true, floor, order);
                }
            }
        }
    }

    fn bump_hall_epoch(&mut self, floor: usize, order: OrderType) {
        match order {
            OrderType::HallUp => {
                self.hall_epoch[floor][HALLUP] = self.hall_epoch[floor][HALLUP].saturating_add(1);
            }
            OrderType::HallDown => {
                self.hall_epoch[floor][HALLDOWN] = self.hall_epoch[floor][HALLDOWN].saturating_add(1);
            }
            OrderType::Cab => {
                eprintln!("Error: Cannot bump hall epoch for a Cab order");
            }
        }
    }

    fn bump_hall_epoch_clear(&mut self, floor: usize, order: OrderType) {
        // Bump by 2 on clear to prevent ties 
        match order {
            OrderType::HallUp => {
                self.hall_epoch[floor][HALLUP] = self.hall_epoch[floor][HALLUP].saturating_add(2);
            }
            OrderType::HallDown => {
                self.hall_epoch[floor][HALLDOWN] = self.hall_epoch[floor][HALLDOWN].saturating_add(2);
            }
            OrderType::Cab => {
                eprintln!("Error: Cannot bump hall epoch for a Cab order");
            }
        }
    }

    fn can_clear_order(&self, floor: usize, dir: usize) -> bool {
        let Some(my_state) = self.elevators.get(&self.my_id) else {
            eprintln!("Current elevator not in state");
            return false;
        };
        if my_state.get_floor() != Some(floor as u8) {
            return false;
        }
        if my_state.get_behavior() != Behaviour::DoorOpen {
            return false;
        }
        let direction = my_state.get_direction();
        match dir {
            HALLUP => direction == Direction::Up || direction == Direction::Stop,
            HALLDOWN => direction == Direction::Down || direction == Direction::Stop,
            _ => false,
        }
    }

    pub fn clear_order(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor cleared");
            return;
        }

        let direction = {
            let local_state = self.get_my_state();
            local_state.clear_cab_request(floor);
            local_state.get_direction()
        };

        let mut clear_up = false;
        let mut clear_down = false;

        match direction {
            Direction::Up => {
                if self.hall_requests[floor as usize][HALLUP] {
                    clear_up = true;
                }
            }
            Direction::Down => {
                if self.hall_requests[floor as usize][HALLDOWN] {
                    clear_down = true;
                }
            }
            Direction::Stop => {
                if self.hall_requests[floor as usize][HALLUP] {
                    clear_up = true;
                } else if self.hall_requests[floor as usize][HALLDOWN] {
                    clear_down = true;
                }
            }
        }

        if clear_up {
            self.bump_hall_epoch_clear(floor as usize, OrderType::HallUp);
            self.set_hall_order(false, floor, OrderType::HallUp);
        }
        if clear_down {
            self.bump_hall_epoch_clear(floor as usize, OrderType::HallDown);
            self.set_hall_order(false, floor, OrderType::HallDown);
        }
    }

    pub fn update_lights(&mut self, driver: &Elevator) {
        for floor in 0..NUM_FLOORS {
            driver.call_button_light(
                floor as u8,
                OrderType::HallUp.into(),
                self.hall_requests[floor][HALLUP],
            );

            driver.call_button_light(
                floor as u8,
                OrderType::HallDown.into(),
                self.hall_requests[floor][HALLDOWN],
            );
        }

        let local_state = self.get_my_state();
        for floor in 0..NUM_FLOORS {
            driver.call_button_light(
                floor as u8,
                OrderType::Cab.into(),
                local_state.get_cab_request(floor as u8),
            );
        }
    }

    pub fn merge_with(&mut self, other: &SystemState) -> bool {
        let mut changed = false;
        for (id, other_state) in &other.elevators {
            if self.dead_elevators.contains(id) && id != &self.my_id {
                continue;
            }

            if id == &self.my_id {
                if !self.sync_complete {
                    changed |= self.get_my_state().recover_from_backup(other_state);
                }
                continue;
            }

            match self.elevators.get(id) {
                None => {
                    self.elevators.insert(id.clone(), other_state.clone());
                    changed = true;
                }
                Some(self_state) => {
                    if other_state.is_newer_than(self_state) {
                        self.elevators.insert(id.clone(), other_state.clone());
                        changed = true;
                    }
                }
            }
        }

        /*
        Merge hall requests for given floor and direction by epoch. If conflicting, we have to manage it.
         */
        for floor in 0..NUM_FLOORS {
            for dir in 0..2 {
                let self_epoch = self.hall_epoch[floor][dir];
                let other_epoch = other.hall_epoch[floor][dir];

                if other_epoch > self_epoch {
                    if self.hall_epoch[floor][dir] != other_epoch
                        || self.hall_requests[floor][dir] != other.hall_requests[floor][dir]
                    {
                        changed = true;
                    }
                    self.hall_epoch[floor][dir] = other_epoch;
                    self.hall_requests[floor][dir] = other.hall_requests[floor][dir];
                    continue;
                }

                if self_epoch > other_epoch {
                    continue;
                }

                let request_self = self.hall_requests[floor][dir];
                let request_other = other.hall_requests[floor][dir];

                if request_self == request_other {
                    continue; 
                }

                let self_clear = !request_self && self.can_clear_order(floor, dir);
                let other_clear = !request_other && other.can_clear_order(floor, dir);

                if self_clear || other_clear {
                    if self.hall_requests[floor][dir] {
                        self.hall_requests[floor][dir] = false;
                        changed = true;
                    }
                } else {
                    if !self.hall_requests[floor][dir] {
                        self.hall_requests[floor][dir] = true;
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}

impl fmt::Display for SystemState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();

        writeln!(
            f,
            "SystemState (id: {}) @ {}.{:03}",
            self.my_id,
            now.as_secs(),
            now.subsec_millis()
        )?;
        writeln!(f, "Hall requests:")?;
        for floor in 0..NUM_FLOORS {
            writeln!(
                f,
                "  Floor {:2}: up={} (e={}), down={} (e={})",
                floor,
                self.hall_requests[floor][HALLUP],
                self.hall_epoch[floor][HALLUP],
                self.hall_requests[floor][HALLDOWN],
                self.hall_epoch[floor][HALLDOWN]
            )?;
        }
        writeln!(f, "Elevators:")?;
        for (id, state) in &self.elevators {
            writeln!(f, "  {}: {:?}", id, state)?;
        }
        Ok(())
    }
}

use crate::types::direction::Direction;
use crate::types::elevator::{Behaviour, ElevatorState};
use crate::{config::NUM_FLOORS, types::orders::OrderType};
use core::fmt;
use driver_rust::elevio::elev::Elevator;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemState {
    my_id: String,
    #[serde(rename = "states")] // for hall_request_assigner
    elevators: HashMap<String, ElevatorState>,
    #[serde(rename = "hallRequests")]
    hall_requests: [[bool; 2]; NUM_FLOORS],
    hall_epoch: [[u64; 2]; NUM_FLOORS],
    #[serde(skip)]
    dead_elevators: HashSet<String>,
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
        }
    }

    pub fn get_my_state(&mut self) -> &mut ElevatorState {
        self.elevators
            .entry(self.my_id.clone())
            // TODO Maybe exit program if not found instead of inserting default state
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

    fn set_hall_order(&mut self, arrive: bool, floor: u8, order: OrderType) {
        match order {
            OrderType::HallUp => self.hall_requests[floor as usize][0] = arrive,
            OrderType::HallDown => self.hall_requests[floor as usize][1] = arrive,
            OrderType::Cab => {
                eprintln!("Should not be called with Cab order")
            }
        }
    }

    pub fn arrive_at_floor(&mut self, floor: u8) {
        let local_state = self.get_my_state();
        local_state.set_floor(floor);

        if floor == 0 || floor == (NUM_FLOORS - 1) as u8 {
            local_state.stop();
        }
    }

    pub fn add_order(&mut self, floor: u8, order: OrderType) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor ordered");
            return;
        }

        match order {
            OrderType::Cab => {
                let local_state = self.get_my_state();
                local_state.add_cab_request(floor);
            }
            OrderType::HallUp => {
                if !self.hall_requests[floor as usize][0] {
                    self.bump_hall_epoch(floor as usize, order);
                    self.set_hall_order(true, floor, order);
                }
            }
            OrderType::HallDown => {
                if !self.hall_requests[floor as usize][1] {
                    self.bump_hall_epoch(floor as usize, order);
                    self.set_hall_order(true, floor, order);
                }
            }
        }
    }

    fn bump_hall_epoch(&mut self, floor: usize, order: OrderType) {
        match order {
            OrderType::HallUp => {
                self.hall_epoch[floor][0] = self.hall_epoch[floor][0].saturating_add(1);
            }
            OrderType::HallDown => {
                self.hall_epoch[floor][1] = self.hall_epoch[floor][1].saturating_add(1);
            }
            OrderType::Cab => {
                eprintln!("Error: Cannot bump hall epoch for a Cab order");
            }
        }
    }

    fn can_clear_order(&self, floor: usize, dir: usize) -> bool {
        // Checks based on elevators own state, if it can clear the order
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
            0 => direction == Direction::Up || direction == Direction::Stop,
            1 => direction == Direction::Down || direction == Direction::Stop,
            _ => false,
        }
    }

    pub fn clear_order(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor cleared");
            return;
        }

        // Scope the borrow to drop local_state before mutating self
        let direction = {
            let local_state = self.get_my_state();
            local_state.clear_cab_request(floor);
            local_state.get_direction()
        };

        let mut clear_up = false;
        let mut clear_down = false;

        match direction {
            Direction::Up => {
                if self.hall_requests[floor as usize][0] {
                    clear_up = true; // Clear UP if it exists
                } else if self.hall_requests[floor as usize][1] {
                    clear_down = true; // Fallback: clear DOWN if no UP exists
                }
            }
            Direction::Down => {
                if self.hall_requests[floor as usize][1] {
                    clear_down = true; // Clear DOWN if it exists
                } else if self.hall_requests[floor as usize][0] {
                    clear_up = true; // Fallback: clear UP if no DOWN exists
                }
            }
            Direction::Stop => {
                // If stopped at a floor, we can clear both directions, but prioritize the one that is actually ordered
                if self.hall_requests[floor as usize][0] {
                    clear_up = true;
                } else if self.hall_requests[floor as usize][1] {
                    clear_down = true;
                }
            }
        }

        if clear_up {
            self.bump_hall_epoch(floor as usize, OrderType::HallUp);
            self.set_hall_order(false, floor, OrderType::HallUp);
        }
        if clear_down {
            self.bump_hall_epoch(floor as usize, OrderType::HallDown);
            self.set_hall_order(false, floor, OrderType::HallDown);
        }
    }

    pub fn update_lights(&mut self, driver: &Elevator) {
        for floor in 0..NUM_FLOORS {
            driver.call_button_light(
                floor as u8,
                OrderType::HallUp.into(),
                self.hall_requests[floor][0],
            );

            driver.call_button_light(
                floor as u8,
                OrderType::HallDown.into(),
                self.hall_requests[floor][1],
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
        // Merge elevator states by sequence number.
        for (id, other_state) in &other.elevators {
            if self.dead_elevators.contains(id) && id != &self.my_id {
                continue;
            }

            if id == &self.my_id {
                changed |= self.get_my_state().recover_from_backup(other_state);
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
        Merge hall requests for given floor and direction by epoch.
        Go through each cell and take the one with higher epoch. If conflicting, we have to manage it.
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
                    continue; // No conflict, same value
                }

                // Equal epoch: check can_clear
                let self_clear = !request_self && self.can_clear_order(floor, dir);
                let other_clear = !request_other && other.can_clear_order(floor, dir);

                if self_clear || other_clear {
                    if self.hall_requests[floor][dir] {
                        self.hall_requests[floor][dir] = false; // Clear the order
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

// Implementation for pretty printing the elevator state to terminal
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
                self.hall_requests[floor][0],
                self.hall_epoch[floor][0],
                self.hall_requests[floor][1],
                self.hall_epoch[floor][1]
            )?;
        }
        writeln!(f, "Elevators:")?;
        for (id, state) in &self.elevators {
            writeln!(f, "  {}: {:?}", id, state)?;
        }
        Ok(())
    }
}

use crate::types::direction::{self, Direction};
use crate::types::elevator::ElevatorState;
use crate::{config::NUM_FLOORS, types::orders::OrderType};
use core::fmt;
use driver_rust::elevio::elev::Elevator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemState {
    my_id: String,
    elevators: HashMap<String, ElevatorState>,
    hall_requests: [[bool; 2]; NUM_FLOORS],
}

impl SystemState {
    pub fn new(id: &str) -> SystemState {
        let mut elevators: HashMap<String, ElevatorState> = HashMap::new();
        elevators.insert(id.to_owned(), ElevatorState::default());

        let hall_requests = [[false; 2]; NUM_FLOORS];

        SystemState {
            my_id: id.to_owned(),
            elevators,
            hall_requests,
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
    }

    pub fn add_order(&mut self, floor: u8, order: OrderType) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor ordered");
            return;
        }
        if order == OrderType::Cab {
            let local_state = self.get_my_state();
            local_state.add_cab_request(floor);
        } else {
            self.set_hall_order(true, floor, order);
        }
    }

    pub fn clear_order(&mut self, floor: u8) {
        if (floor as usize) >= NUM_FLOORS {
            eprintln!("Undefined floor cleared");
            return;
        }

        let local_state = self.get_my_state();
        local_state.clear_cab_request(floor);

        let direction = local_state.get_direction();

        match direction {
            Direction::Up => self.set_hall_order(false, floor, OrderType::HallUp),
            Direction::Down => self.set_hall_order(false, floor, OrderType::HallDown),
            Direction::Stop => {
                // FIXME: Read specs and see the specific behavior for when both up and down are pressed in idle elevator
                self.set_hall_order(false, floor, OrderType::HallUp);
                self.set_hall_order(false, floor, OrderType::HallDown);
            }
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

    pub fn merge_with(&mut self, other: &SystemState) {
        // The authority of clearing orders are given to an elevator that is at the correct floor and direction and door open
    }
}

// Implementation for pretty printing the elevator state to terminal
impl fmt::Display for SystemState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "SystemState (id: {})", self.my_id)?;
        writeln!(f, "Hall requests:")?;
        for floor in 0..NUM_FLOORS {
            writeln!(
                f,
                "  Floor {:2}: up={}, down={}",
                floor, self.hall_requests[floor][0], self.hall_requests[floor][1]
            )?;
        }
        writeln!(f, "Elevators:")?;
        for (id, state) in &self.elevators {
            writeln!(f, "  {}: {:?}", id, state)?;
        }
        Ok(())
    }
}

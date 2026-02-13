use crate::types::elevator::ElevatorState;
use crate::{config::NUM_FLOORS, types::orders::OrderType};
use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemState {
    pub my_id: String,
    pub elevators: HashMap<String, ElevatorState>,
    pub hall_requests: [[bool; 2]; NUM_FLOORS],
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

    pub fn get_my_state(&mut self) -> std::option::Option<&mut ElevatorState> {
        self.elevators.get_mut(&self.my_id)
    }

    pub fn arrive_at_floor(&mut self, floor: u8) {
        if let Some(this_elevator_state) = self.elevators.get_mut(&self.my_id) {
            this_elevator_state.set_floor(floor);
        } else {
            eprintln!("Current elevator not in state");
        }
    }

    pub fn add_order(&mut self, floor: u8, order: OrderType) {
        match order {
            OrderType::Cab => {
                if (floor as usize) >= NUM_FLOORS {
                    eprintln!("Undefined floor ordered");
                    return;
                }

                if let Some(this_elevator_state) = self.elevators.get_mut(&self.my_id) {
                    this_elevator_state.set_cab_request(floor);
                } else {
                    eprintln!("Current elevator not in state");
                }
            }
            OrderType::HallUp => {
                if (floor as usize) < NUM_FLOORS {
                    self.hall_requests[floor as usize][0] = true;
                }
            }
            OrderType::HallDown => {
                if (floor as usize) < NUM_FLOORS {
                    self.hall_requests[floor as usize][1] = true;
                }
            }
        }
    }

    pub fn clear_orders_at_floor(
        &mut self,
        floor: u8,
        elevator_driver: &driver_rust::elevio::elev::Elevator,
    ) {
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

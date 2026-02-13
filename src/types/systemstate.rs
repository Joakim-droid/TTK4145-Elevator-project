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

        // epoch: Represent a counter that increments based on button presses. Used to determine which state is more recent for mergining.
        // active: Represents if an epoch is valid

        /*
        PSEUDOCODE // ALGORITHM FOR MERGING

        hall_epoch: [[u64; 2]; NUM_FLOORS] // up and down
        hall_orders: [[bool; 2]; NUM_FLOORS] // up and down

        sequnece: Counter: local operations // used to indicate how recent a state is

        // When will it change?: 

        if (change in local elevator state) {
            increment sequence

        if (hall order) {
            increment epoch for that floor and direction
            make request active
            }

        if (hall order complete) {
            hall_orders[floor][dir] = false
            }
        
        if elevator.seq > other.seq {
            keep the more recent state
            }

        // How to merge:
        
        // Updating own state:
        for state, id in other.elevators{

            if elevator id does not exist in self.elevators {
                add elevator to state // add new elevator to state
            } 
            else {
                if self.seq > other.seq {
                    keep the more recent state
                    }
            }
        
        // Updating hall orders:
        for floor in floors {
            for direction in [up, down] {
                look at order and epoch for that floor and direction in both states

                if other.epoch > self.epoch 
                    update both orders and epoch to other since it is more relevant

                else if self.epoch > other.epoch
                    keep self
                
                if self.orders[floor][direction] != other.orders[floor][direction]
                    clear if allowed // helper function

                
        // CLEARING ORDERS: 

        CLEAR_ORDER(floor, direction){
        // Finds out if order at (floor, direction) can be cleared
        


    }
       


        
         */
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

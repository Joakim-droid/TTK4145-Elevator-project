use crate::{
    config::DOOR_OPEN_DURATION,
    types::elevator::{Behaviour, ElevatorState},
};
use driver_rust::elevio::elev::{DIRN_DOWN, DIRN_STOP, DIRN_UP, Elevator};
use std::time::{Duration, SystemTime};

fn find_direction(current_floor: u8, goal_floor: u8) -> u8 {
    if goal_floor > current_floor {
        DIRN_UP
    } else {
        DIRN_DOWN
    }
}

/// Executes the necessary actions to serve the goal
pub fn step(
    elevator_driver: &Elevator,
    elevator_state: &mut ElevatorState, // Timer is inside here now
    goal: Option<u8>,
) {
    if goal.is_none() {
        elevator_driver.motor_direction(DIRN_STOP);
        elevator_state.stop();
        return;
    }

    match elevator_state.get_behavior() {
        Behaviour::Idle => {
            let current_floor = elevator_state.get_floor();

            if current_floor.is_none() || goal.is_none() {
                // TODO: Maybe handle this error
                eprintln!("Elevator is stuck between floors");
                return;
            }

            let current_floor = current_floor.unwrap();
            let goal_floor = goal.unwrap();

            if current_floor == goal_floor {
                elevator_state.open_door();
                elevator_driver.door_light(true);
            } else {
                let direction = find_direction(current_floor, goal_floor);

                elevator_driver.motor_direction(direction);
                elevator_state.set_direction(direction);
            }
        }

        Behaviour::Moving => {
            let current_floor = elevator_state.get_floor();

            if current_floor.is_some() && goal.is_none() {
                // No orders to serve
                elevator_state.stop();
                elevator_driver.motor_direction(DIRN_STOP);
                return;
            }

            if Some(current_floor) == Some(goal) {
                elevator_driver.motor_direction(DIRN_STOP);
                elevator_state.open_door();
                elevator_driver.door_light(true);
            }
        }

        Behaviour::DoorOpen => {
            if let Some(goal_floor) = goal
                && let Some(current) = elevator_state.get_floor()
                && goal_floor == current
            {
                // Resets timer if a button on the current floor is pressed while the door is open
                elevator_state.open_door();
                return;
            }

            let door_timer = elevator_state.get_door_timer();

            if door_timer.is_none() {
                eprintln!("Door should not be open with no timer");
                return;
            }

            let door_timer = door_timer.unwrap();

            // TODO: May have to handle elapsed function failing
            if door_timer.elapsed().unwrap_or(Duration::ZERO) >= DOOR_OPEN_DURATION {
                elevator_driver.door_light(false);
                elevator_state.close_door();
            }
        }
    }
}

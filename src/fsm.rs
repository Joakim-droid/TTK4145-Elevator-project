use crate::{
    config::DOOR_OPEN_DURATION,
    types::{
        elevator::{Behaviour, ElevatorState},
        event::Event,
    },
};
use crossbeam_channel::Sender;
use driver_rust::elevio::elev::{DIRN_DOWN, DIRN_STOP, DIRN_UP, Elevator};

fn find_direction(current_floor: u8, goal_floor: u8) -> u8 {
    if goal_floor > current_floor {
        DIRN_UP
    } else {
        DIRN_DOWN
    }
}

fn spawn_door_timer(timer_id: u64, event_tx: Sender<Event>) {
    std::thread::spawn(move || {
        std::thread::sleep(DOOR_OPEN_DURATION);
        event_tx.send(Event::DoorOpenTimeOut(timer_id)).unwrap();
    });
}

/// Executes the necessary actions to serve the goal
pub fn step(
    elevator_driver: &Elevator,
    elevator_state: &mut ElevatorState,
    goal: Option<u8>,
    event_tx: Sender<Event>,
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

            if current_floor == goal_floor
                && let Some(timer_id) = elevator_state.open_door()
            {
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
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

            if Some(current_floor) == Some(goal)
                && let Some(timer_id) = elevator_state.open_door()
            {
                elevator_driver.door_light(true);
                elevator_driver.motor_direction(DIRN_STOP);
                spawn_door_timer(timer_id, event_tx.clone());
            }
        }

        Behaviour::DoorOpen => {
            let current_floor = elevator_state.get_floor();

            if current_floor.is_none() {
                eprintln!(
                    "ERROR: Elevator is in DoorOpen state, but floor sensor is None (Between floors)."
                );
                return;
            }

            let current_floor = current_floor.unwrap();

            if goal.is_none() {
                // No orders, waiting for door time out
                return;
            }

            let goal_floor = goal.unwrap();

            if goal_floor != current_floor {
                // New goal exist, but must wait for door timeout.
                return;
            }

            // Resets timer if a button on the current floor is pressed while the door is open
            if let Some(timer_id) = elevator_state.open_door() {
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
            }
        }
    }
}

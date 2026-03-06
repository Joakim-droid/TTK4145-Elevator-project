use crate::{
    config::DOOR_OPEN_DURATION,
    types::{direction::Direction, elevator::Behaviour, event::Event, systemstate::SystemState},
};
use crossbeam_channel::Sender;
use driver_rust::elevio::elev::Elevator;

fn find_direction(current_floor: u8, goal_floor: u8) -> Direction {
    if goal_floor > current_floor {
        Direction::Up
    } else {
        Direction::Down
    }
}

pub fn spawn_door_timer(timer_id: u64, event_tx: Sender<Event>) {
    std::thread::spawn(move || {
        std::thread::sleep(DOOR_OPEN_DURATION);
        event_tx.send(Event::DoorOpenTimeOut(timer_id)).unwrap();
    });
}

/// Executes the necessary actions to serve the goal
pub fn step(
    elevator_driver: &Elevator,
    system_state: &mut SystemState,
    goal: Option<u8>,
    event_tx: Sender<Event>,
) {
    let local_elevator_state = system_state.get_my_state();

    if local_elevator_state.is_obstructed() {
        if local_elevator_state.get_behavior() == Behaviour::Moving {
            elevator_driver.motor_direction(Direction::Stop.into());
            local_elevator_state.stop();
        }

        if let Some(new_id) = local_elevator_state.open_door() {
            elevator_driver.door_light(true);
            spawn_door_timer(new_id, event_tx.clone());
        }
        return;
    }

    if goal.is_none() {
        elevator_driver.motor_direction(Direction::Stop.into());
        local_elevator_state.stop();
        return;
    }

    if local_elevator_state.is_emergency_stop() {
        elevator_driver.motor_direction(Direction::Stop.into());
        local_elevator_state.stop();
        return;
    }

    match local_elevator_state.get_behavior() {
        Behaviour::Idle => {
            let current_floor = local_elevator_state.get_floor();

            if current_floor.is_none() || goal.is_none() {
                // TODO: Maybe handle this error
                eprintln!("Elevator is stuck between floors");
                return;
            }

            let current_floor = current_floor.unwrap();
            let goal_floor = goal.unwrap();

            if current_floor == goal_floor
                && let Some(timer_id) = local_elevator_state.open_door()
            {
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
                // TODO: Clear orders at this floor,
                system_state.clear_order(current_floor);
            } else {
                let direction = find_direction(current_floor, goal_floor);

                elevator_driver.motor_direction(direction.into());
                local_elevator_state.set_direction(direction);
            }
        }

        Behaviour::Moving => {
            let current_floor = local_elevator_state.get_floor();

            if current_floor.is_none() {
                return;
            }

            let current_floor = current_floor.unwrap();

            if goal.is_none() {
                // No orders to serve
                local_elevator_state.stop();
                elevator_driver.motor_direction(Direction::Stop.into());
                return;
            }

            let goal = goal.unwrap();

            if current_floor == goal
                && let Some(timer_id) = local_elevator_state.open_door()
            {
                elevator_driver.door_light(true);
                elevator_driver.motor_direction(Direction::Stop.into());
                spawn_door_timer(timer_id, event_tx.clone());
                // TODO: Clear orders at this floor
                system_state.clear_order(current_floor);
            }
        }

        Behaviour::DoorOpen => {
            let current_floor = local_elevator_state.get_floor();

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
            if let Some(timer_id) = local_elevator_state.open_door() {
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
                system_state.clear_order(current_floor);
            }
        }
    }
}

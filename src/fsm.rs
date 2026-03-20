//! Finite-state machine for elevator motion, door handling, and order servicing decisions.

use crate::logger;
use crate::{
    config::DOOR_OPEN_DURATION,
    logger::LogEvent,
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
        let _ = event_tx.send(Event::DoorOpenTimeOut(timer_id));
    });
}

/// Executes the necessary actions to serve the goal
pub fn execute_state_transition(
    elevator_driver: &Elevator,
    system_state: &mut SystemState,
    goal: Option<u8>,
    event_tx: Sender<Event>,
    triggered_by_button_press: bool,
) {
    let local_elevator_state = system_state.get_my_state();

    if local_elevator_state.get_behavior() == Behaviour::Idle {
        elevator_driver.motor_direction(Direction::Stop.into());
    }

    let is_floor = elevator_driver.floor_sensor().is_some();

    if local_elevator_state.is_emergency_stop() {
        elevator_driver.motor_direction(Direction::Stop.into());
        local_elevator_state.stop();
        return;
    }

    if local_elevator_state.get_behavior() == Behaviour::Moving && !is_floor {
        if let Some(_goal) = goal {
            let current_direction = local_elevator_state.get_direction();

            elevator_driver.motor_direction(current_direction.into());
        }
        return;
    }

    if local_elevator_state.is_obstructed() && is_floor{
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

    match local_elevator_state.get_behavior() {
        Behaviour::Idle => {
            let current_floor = local_elevator_state.get_floor();

            if current_floor.is_none() || goal.is_none() {
                eprintln!("Elevator is stuck between floors");
                return;
            }

            let current_floor = current_floor.unwrap();
            let goal_floor = goal.unwrap();

            let should_serve_here = current_floor == goal_floor || local_elevator_state.get_cab_request(current_floor);

            if should_serve_here && is_floor
                && let Some(timer_id) = local_elevator_state.open_door()
            {
                logger::log(LogEvent::DoorOpened {
                    floor: current_floor,
                });
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
                system_state.clear_order(current_floor);
                logger::log(LogEvent::OrderCleared {
                    floor: current_floor,
                });
            } else {
                let direction = find_direction(current_floor, goal_floor);

                logger::log(LogEvent::MotorStart {
                    floor: current_floor,
                    direction,
                    goal: goal_floor,
                });
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
                local_elevator_state.stop();
                elevator_driver.motor_direction(Direction::Stop.into());
                return;
            }

            let goal = goal.unwrap();

            let should_serve_here =
                current_floor == goal || local_elevator_state.get_cab_request(current_floor);

            if should_serve_here
                && let Some(timer_id) = local_elevator_state.open_door()
            {
                logger::log(LogEvent::DoorOpened {
                    floor: current_floor,
                });
                elevator_driver.motor_direction(Direction::Stop.into());
                elevator_driver.door_light(true);
                spawn_door_timer(timer_id, event_tx.clone());
                system_state.clear_order(current_floor);
                logger::log(LogEvent::OrderCleared {
                    floor: current_floor,
                });
            } else {
                let desired_direction = find_direction(current_floor, goal);
                if desired_direction != local_elevator_state.get_direction() {
                    logger::log(LogEvent::MotorReverse {
                        floor: current_floor,
                        from: local_elevator_state.get_direction(),
                        to: desired_direction,
                        goal,
                    });
                    elevator_driver.motor_direction(desired_direction.into());
                    local_elevator_state.set_direction(desired_direction);
                }
            }
        }

        Behaviour::DoorOpen => {
            elevator_driver.motor_direction(Direction::Stop.into());

            let current_floor = local_elevator_state.get_floor();

            if current_floor.is_none() {
                eprintln!(
                    "ERROR: Elevator is in DoorOpen state, but floor sensor is None (Between floors)."
                );
                return;
            }

            let current_floor = current_floor.unwrap();

            if goal.is_none() {
                return;
            }

            let goal_floor = goal.unwrap();

            if goal_floor != current_floor {
                return;
            }

            if triggered_by_button_press {
                if let Some(timer_id) = local_elevator_state.open_door() {
                    elevator_driver.door_light(true);
                    spawn_door_timer(timer_id, event_tx.clone());
                    system_state.clear_order(current_floor);
                }
            }
        }
    }
}


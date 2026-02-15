use crate::types::elevator::{Behaviour, ElevatorState};
use driver_rust::elevio::elev::Elevator;
use std::time::Duration;

const DOOR_OPEN_DURATION: Duration = Duration::from_millis(3000);

pub fn step(
    elevator_driver: &Elevator,
    elevator_state: &mut ElevatorState, // Timer is inside here now
    goal: Option<u8>,
) -> bool {
    let door_just_opened = false;

    match elevator_state.behaviour {
        Behaviour::Idle => {}

        Behaviour::Moving => {}

        Behaviour::DoorOpen => {}
    }

    door_just_opened
}

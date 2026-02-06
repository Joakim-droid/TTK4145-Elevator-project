use crate::{
    config::NUM_FLOORS,
    types::{
        elevator::{Behaviour, ElevatorState},
        systemstate::SystemState,
    },
};

fn time_to_serve_request(e: &ElevatorState, req_floor: u8) -> u32 {
    let mut duration = 0;

    if let Some(f) = e.floor {
        duration += (f as i32 - req_floor as i32).abs() as u32;
    }
    duration
}

pub fn decide_next_order(system_state: &SystemState) -> Option<u8> {
    let my_id = &system_state.my_id;

    None
}

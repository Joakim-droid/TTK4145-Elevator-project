use crate::{
    config::NUM_FLOORS,
    types::{orders::OrderType, systemstate::SystemState},
};
use std::collections::HashMap;
use std::process::Command;

pub fn decide_next_order(system_state: &SystemState) -> Option<u8> {
    // Serialize data
    let system_state_clone = system_state.clone();
    let mut serialized_system_state = serde_json::to_value(&system_state_clone).unwrap();

    serialized_system_state
        .as_object_mut()
        .unwrap()
        .remove("my_id");
    let hall_request_assigner_json =
        serde_json::to_string(&serialized_system_state).expect("Failed to serialize data");

    // Run the executable with serialized_data as input
    let program_out = Command::new("./execs/hall_request_assigner")
        .arg("-i")
        .arg(&hall_request_assigner_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

    let mut assigned_hall_requests = vec![vec![false; 2]; NUM_FLOORS];
    if program_out.status.success() {
        let hall_request_assigner_output_str =
            String::from_utf8(program_out.stdout).expect("Invalid UTF-8 hra_output");
        let hall_request_assigner_output_value = serde_json::from_str::<
            HashMap<String, Vec<Vec<bool>>>,
        >(&hall_request_assigner_output_str)
        .expect("Failed to deserialize");
        // just to see what the assigner returned
        println!("{}", hall_request_assigner_output_str);

        for (id, hall_requests) in hall_request_assigner_output_value.iter() {
            if id == &system_state_clone.get_my_id() {
                for floor in 0..NUM_FLOORS {
                    assigned_hall_requests[floor][OrderType::HallUp as usize] =
                        hall_requests[floor][OrderType::HallUp as usize];
                    assigned_hall_requests[floor][OrderType::HallDown as usize] =
                        hall_requests[floor][OrderType::HallDown as usize];
                }
            }
        }
    }
    None
}

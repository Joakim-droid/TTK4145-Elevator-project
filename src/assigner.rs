use std::process::Command;
use std::collections::HashMap;
use crate::{
    config::NUM_FLOORS,
    types::{
        elevator::{ElevatorState},
        systemstate::SystemState,
        orders::OrderType
    },
};

fn time_to_serve_request(e: &ElevatorState, req_floor: u8) -> u32 {
    let mut duration = 0;
    duration
}

pub fn decide_next_order(system_state: &SystemState) -> Option<u8> {
    // Serialize data
    let system_state_clone = system_state.clone();
    let mut serialized_system_state= serde_json::to_value(&system_state_clone).unwrap();
      

    serialized_system_state.as_object_mut().unwrap().remove("my_id");
    let hall_request_assigner_json = serde_json::to_string(&serialized_system_state).expect("Failed to serialize data");

        // Run the executable with serialized_data as input
        let program_out = Command::new("../execs/hall_request_assigner")
            .arg("-i")
            .arg(&hall_request_assigner_json)
            .output()
            .expect("Failed to execute hall_request_assigner");

        let mut assigned_hall_requests = vec![vec![false; 2]; NUM_FLOORS];
        if program_out.status.success() {
            let hall_request_assigner_output_str = String::from_utf8(program_out.stdout).expect("Invalid UTF-8 hra_output");
            let hall_request_assigner_output_value = serde_json::from_str::<HashMap<String, Vec<Vec<bool>>>>(&hall_request_assigner_output_str)
                    .expect("Failed to deserialize");
            // just to see what the assigner returned
            println!("{}", hall_request_assigner_output_str);
            
            for (id, hall_requests) in hall_request_assigner_output_value.iter() {
                if id == &system_state_clone.my_id {
                    for floor in 0..NUM_FLOORS {
                        assigned_hall_requests[floor as usize][OrderType::HallUp as usize] = hall_requests[floor as usize][OrderType::HallUp  as usize];
                        assigned_hall_requests[floor as usize][OrderType::HallDown  as usize] = hall_requests[floor as usize][OrderType::HallDown  as usize];
                    }
                }
            }
            // map floor and direction info into u8
            assigned_hall_requests.iter().enumerate().find_map(|(floor, pair)| {
                match pair.as_slice() {
                    [true, _] => Some((floor * 2) as u8),        // up
                    [_, true] => Some((floor * 2 + 1) as u8),    // down
                    _ => None,
                }
            })
        } else {
            eprint!("Error executing hall_request_assigner");
            std::process::exit(1);
        }

}

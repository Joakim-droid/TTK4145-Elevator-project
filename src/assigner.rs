use crate::types::systemstate::SystemState;
use std::collections::HashMap;
use std::process::Command;

pub fn decide_next_order(system_state: &SystemState) -> Option<u8> {
    // Serialize data
    let mut system_state_clone = system_state.clone();

    let dead_peers = system_state.get_dead_elevators().clone();
    for dead_id in dead_peers {
        system_state_clone.remove_elevator_record(&dead_id);
    }

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
        .arg("--includeCab")
        .output()
        .expect("Failed to execute hall_request_assigner");

    if program_out.status.success() {
        let hall_request_assigner_output_str =
            String::from_utf8(program_out.stdout).expect("Invalid UTF-8 hra_output");
        let hall_request_assigner_output_value = serde_json::from_str::<
            HashMap<String, Vec<Vec<bool>>>,
        >(&hall_request_assigner_output_str)
        .expect("Failed to deserialize");
        // just to see what the assigner returned
        // println!("{}", hall_request_assigner_output_str);

        let my_id = system_state_clone.get_my_id();
        if let Some(my_orders) = hall_request_assigner_output_value.get(&my_id) {
            for (floor, orders) in my_orders.iter().enumerate() {
                if orders.iter().any(|&active| active) {
                    return Some(floor as u8);
                }
            }
        }

        None
    } else {
        let error_msg = String::from_utf8_lossy(&program_out.stderr);
        eprintln!("Error executing hall_request_assigner: {}", error_msg);
        None
    }
}

//! Integrates with the external hall request assigner and picks the local elevator's next goal.

use crate::logger::{self, LogEvent};
use crate::types::systemstate::SystemState;
use std::collections::HashMap;
use std::process::Command;


pub fn decide_next_order(system_state: &SystemState) -> Option<u8> {

    let mut system_state_clone = system_state.clone();

    let dead_peers = system_state.get_dead_elevators().clone();
    for dead_id in dead_peers {
        system_state_clone.remove_elevator_record(&dead_id);
    }

    // Remove peers that cannot serve orders: obstructed or in emergency stop.
    let my_id = system_state_clone.get_my_id();
    let unavailable_peers: Vec<String> = system_state_clone
        .get_elevator_ids()
        .into_iter()
        .filter(|id| *id != my_id) 
        .filter(|id| {
            system_state_clone
                .get_elevator_state(id)
                .map(|s| s.is_obstructed() || s.is_emergency_stop())
                .unwrap_or(false)
        })
        .collect();
    for id in unavailable_peers {
        system_state_clone.remove_elevator_record(&id);
    }

    let mut serialized_system_state = serde_json::to_value(&system_state_clone).unwrap();

    serialized_system_state
        .as_object_mut()
        .unwrap()
        .remove("my_id");

    let hall_request_assigner_json =
        serde_json::to_string(&serialized_system_state).expect("Failed to serialize data");

    logger::log(LogEvent::AssignerInput {
        json: &hall_request_assigner_json,
    });

    // Calling the external assigner program.
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

        let my_id = system_state_clone.get_my_id();
        if let Some(my_orders) = hall_request_assigner_output_value.get(&my_id) {
            for (floor, orders) in my_orders.iter().enumerate() {
                if orders.iter().any(|&active| active) {
                    let goal = Some(floor as u8);
                    logger::log(LogEvent::AssignerDecision { goal });
                    return goal;
                }
            }
        }

        logger::log(LogEvent::AssignerDecision { goal: None });
        None
    } else {
        let error_msg = String::from_utf8_lossy(&program_out.stderr);
        eprintln!("Error executing hall_request_assigner: {}", error_msg);
        None
    }
}

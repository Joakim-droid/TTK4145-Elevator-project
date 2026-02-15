use std::process::Command;
use crate::{
    // config::NUM_FLOORS,
    types::{
        elevator::{ElevatorState},
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


        if program_out.status.success() {
            let hra_output_str = String::from_utf8(program_out.stdout).expect("Invalid UTF-8 hra_output");
            println!("{}", hra_output_str)
        } 
        None

}

use crate::{
    config::NUM_FLOORS,
    hardware::{spawn_button_poller, spawn_floor_poller},
    types::{event::Event, systemstate::SystemState},
};
use crossbeam_channel::{self as cbc, select};
use driver_rust::elevio::elev::Elevator;
mod config;
mod hardware;
mod types;

fn main() {
    let elevator_address = "localhost:15658";

    let elevator_driver =
        Elevator::init(elevator_address, NUM_FLOORS as u8).expect("Error connecting to Elevator");

    let mut system_state = SystemState::new(elevator_address);

    let (event_tx, event_rx) = cbc::unbounded::<Event>();

    spawn_floor_poller(&elevator_driver, event_tx.clone());
    spawn_button_poller(&elevator_driver, event_tx.clone());

    loop {
        select! {
            recv(event_rx) -> event => {
                match event {
                    Ok(Event::FloorReached(floor)) => {
                        // system_state.arrive_at_floor(floor);
                        // elevator_driver.floor_indicator(floor);
                    },

                    Ok(Event::ButtonPressed(floor,order)) => {

                        // Send button pressed update
                        // Recieve acknowledge
                        // Each elevator update state separate

                    },

                    Ok(Event::PeerUpdate(update)) => {
                        let current_peers = update.peers;

                        // Can maybe be done in the recieved state merge function
                        for _ in current_peers {
                            // remove or add peers to systemstate
                        };
                    },

                    Err(_) => println!("Error"),
                }
                // state_to_broadcast_tx.send(system_state.clone()).unwrap();
            }

        }
    }
}

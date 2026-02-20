use std::net::UdpSocket;

use crate::{
    config::NUM_FLOORS,
    hardware::{
        spawn_button_poller, spawn_floor_poller, spawn_obstruction_poller, spawn_stop_button_poller,
    },
    network::{spawn_peer_discovery, spawn_recieve_thread, spawn_send_thread},
    types::{direction::Direction, elevator::Behaviour, event::Event, systemstate::SystemState},
};
use crossbeam_channel::{self as cbc, select};
use driver_rust::elevio::elev::Elevator;
mod assigner;
mod config;
mod fsm;
mod hardware;
mod network;
mod types;

fn main() {
    let elevator_address = "localhost:15658".to_string();

    let elevator_driver =
        Elevator::init(&elevator_address, NUM_FLOORS as u8).expect("Error connecting to Elevator");

    let mut system_state = SystemState::new(&elevator_address);

    let (event_tx, event_rx) = cbc::unbounded::<Event>();

    spawn_floor_poller(&elevator_driver, event_tx.clone());
    spawn_button_poller(&elevator_driver, event_tx.clone());
    spawn_obstruction_poller(&elevator_driver, event_tx.clone());
    spawn_stop_button_poller(&elevator_driver, event_tx.clone());

    let (state_to_broadcast_tx, state_to_broadcast_rx) = cbc::unbounded::<SystemState>();
    let (peer_state_tx, peer_state_rx) = cbc::unbounded::<SystemState>();

    let internal_port = 5001;
    let external_port = 5000;

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", internal_port)).expect("Failed to bind");
    let send_socket = socket.try_clone().unwrap();

    spawn_recieve_thread(socket, peer_state_tx);
    spawn_send_thread(send_socket, state_to_broadcast_rx, external_port);
    spawn_peer_discovery(elevator_address.clone(), event_tx.clone());

    loop {
        select! {
            recv(peer_state_rx) -> msg => {
                println!("Received state from network");
                if let Ok(fetched_state) = msg {
                    system_state.merge_with(&fetched_state);
                    // println!("{system_state}")
                    let order_floor = assigner::decide_next_order(&system_state);

                    fsm::step(
                        &elevator_driver,
                        &mut system_state,
                        order_floor,
                        event_tx.clone()
                    );
                }
                system_state.update_lights(&elevator_driver);
            }

            recv(event_rx) -> event => {
                match event {
                    Ok(Event::FloorReached(floor)) => {
                        system_state.arrive_at_floor(floor);

                        // FIXME: Move this elsewhere
                        let my_state = system_state.get_my_state();
                        if my_state.get_behavior() == Behaviour::Idle {
                            elevator_driver.motor_direction(Direction::Stop.into());
                        }

                        let order_floor = assigner::decide_next_order(&system_state);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            order_floor,
                            event_tx.clone()
                        );

                    },

                    Ok(Event::ButtonPressed(floor,order)) => {
                        system_state.add_order(floor, order);

                        let next_order =  assigner::decide_next_order(&system_state);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            next_order,
                            event_tx.clone()
                        );
                    },

                    Ok(Event::PeerUpdate(update)) => {
                        let current_peers = update.peers;
                        // TODO: what is needed for the assigner
                        // Can maybe be done in the recieved state merge function
                        for _ in current_peers {
                            // remove or add peers to systemstate
                            // Mark as dead or alive

                        };
                    },

                    Ok(Event::DoorOpenTimeOut(timer_id)) => {
                        let my_state = system_state.get_my_state();


                        if my_state.get_current_timer_id() == timer_id {
                            elevator_driver.door_light(false);
                            my_state.close_door();
                            let next_order = assigner::decide_next_order(&system_state);

                            fsm::step(&elevator_driver, &mut system_state, next_order, event_tx.clone());
                        } else {
                            println!("Ignored stale timer event (ID: {})", timer_id);
                        }
                    },

                    Ok(Event::Obstructed(obstructed)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_obstruction(obstructed);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            None,
                            event_tx.clone()
                        );
                    },

                    Ok(Event::EmergencyStop(is_stopped)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_emergency_stop(is_stopped);
                        elevator_driver.stop_button_light(is_stopped);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            None,
                            event_tx.clone()
                        );
                    }

                    Err(_) => println!("Error"),
                }
                system_state.update_lights(&elevator_driver);
                state_to_broadcast_tx.send(system_state.clone()).unwrap();
            }
        }
    }
}

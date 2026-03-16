use crate::{
    config::{NUM_FLOORS, PEER_DISCOVERY_BCAST_PORT},
    hardware::{
        initialize_elevator_position, spawn_button_poller, spawn_floor_poller,
        spawn_obstruction_poller, spawn_stop_button_poller,
    },
    network::{spawn_peer_discovery, spawn_state_broadcast},
    types::{direction::Direction, elevator::Behaviour, event::Event, systemstate::SystemState},
};
use crossbeam_channel::{self as cbc, select};
use driver_rust::elevio::elev::Elevator;
use std::io::Write;
mod assigner;
mod config;
mod fsm;
mod hardware;
mod network;
mod parser;
mod types;

fn main() {
    // If any worker thread panics (e.g. simulator disconnect), terminate the whole node.
    // Otherwise peer heartbeat can stay alive and prevent dead-elevator takeover.
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("Fatal panic, shutting down node: {}", panic_info);
        std::process::exit(1);
    }));

    let (my_id, sim_port, bcast_port) = parser::parse();
    let elevator_address = format!("localhost:{}", sim_port);

    println!(
        "Starting elevator '{}' connecting to simulator on {} (broadcast_port={})",
        my_id, elevator_address, bcast_port
    );
    // TODO: Used for debugging in test script, remove later
    std::io::stdout().flush().unwrap();

    let elevator_driver =
        Elevator::init(&elevator_address, NUM_FLOORS as u8).expect("Error connecting to Elevator");
    let mut system_state = SystemState::new(&my_id);

    let (event_tx, event_rx) = cbc::unbounded::<Event>();
    let (state_to_broadcast_tx, state_to_broadcast_rx) = cbc::unbounded::<SystemState>();
    let (peer_state_tx, peer_state_rx) = cbc::unbounded::<SystemState>();

    initialize_elevator_position(&elevator_driver, &mut system_state);

    spawn_floor_poller(&elevator_driver, event_tx.clone());
    spawn_button_poller(&elevator_driver, event_tx.clone());
    spawn_obstruction_poller(&elevator_driver, event_tx.clone());
    spawn_stop_button_poller(&elevator_driver, event_tx.clone());
    spawn_peer_discovery(my_id.clone(), event_tx.clone(), PEER_DISCOVERY_BCAST_PORT);
    spawn_state_broadcast(
        my_id.clone(),
        bcast_port,
        state_to_broadcast_rx,
        peer_state_tx,
    );

    println!("Initial state:");
    println!("{system_state}");
    // TODO: Used for debugging in test script, remove later
    std::io::stdout().flush().unwrap();

    loop {
        select! {
            recv(peer_state_rx) -> msg => {
                if let Ok(fetched_state) = msg {
                    if system_state.merge_with(&fetched_state) {
                        println!("Received state from network");
                        println!("{system_state}");
                        // TODO: Used for debugging in test script, remove later
                        std::io::stdout().flush().unwrap();
                        let order_floor = assigner::decide_next_order(&system_state);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            order_floor,
                            event_tx.clone()
                        );
                        system_state.update_lights(&elevator_driver);
                        state_to_broadcast_tx.send(system_state.clone()).unwrap();
                    }
                }
            }

            recv(event_rx) -> event => {
                match event {
                    Ok(Event::FloorReached(floor)) => {
                        println!("[EVENT] floor_reached floor={}", floor);
                        system_state.arrive_at_floor(floor);
                        elevator_driver.floor_indicator(floor);

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
                        if let Some(id) = &update.new {
                            system_state.peer_new(id);
                        }

                        for id in &update.lost {
                            println!("Elevator dead: {}", id);
                            system_state.peer_lost(id);
                        }

                        let next_order = assigner::decide_next_order(&system_state);
                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            next_order,
                            event_tx.clone()
                        );
                    },

                    Ok(Event::DoorOpenTimeOut(timer_id)) => {
                        let my_state = system_state.get_my_state();

                        // If obstructed, ignore timeout
                        if my_state.is_obstructed() {
                            if my_state.get_current_timer_id() == timer_id {
                                if let Some(new_id) = my_state.open_door() {
                                    elevator_driver.door_light(true);
                                    fsm::spawn_door_timer(new_id, event_tx.clone());
                                }
                            }
                            return;
                        }

                        if my_state.get_current_timer_id() == timer_id {
                            elevator_driver.door_light(false);
                            my_state.close_door();

                            let next_order = assigner::decide_next_order(&system_state);
                            fsm::step(
                                &elevator_driver, 
                                &mut system_state, 
                                next_order, 
                                event_tx.clone()
                            );
                        } else {
                            println!("Ignored stale timer event (ID: {})", timer_id);
                        }

                    },

                    Ok(Event::Obstructed(obstructed)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_obstruction(obstructed);

                        if obstructed {
                            // Keep door open and invalidate any pending timeout
                            if let Some(new_id) = my_state.open_door() {
                                elevator_driver.door_light(true);
                            }
                        } else {
                            if let Some(new_id) = my_state.open_door() {
                                elevator_driver.door_light(true);
                                fsm::spawn_door_timer(new_id, event_tx.clone());
                            }
                        }

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
                    Err(_) => println!("Error in event loop"),
                }
                system_state.update_lights(&elevator_driver);
                println!("Local event handled");
                println!("{system_state}");
                // TODO: Used for debugging in test script, remove later
                std::io::stdout().flush().unwrap();
                state_to_broadcast_tx.send(system_state.clone()).unwrap();
            }
        }
    }
}

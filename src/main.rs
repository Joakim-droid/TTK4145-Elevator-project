use crate::{
    config::{NUM_FLOORS, PEER_DISCOVERY_BCAST_PORT},
    hardware::{
        initialize_elevator_position, spawn_button_poller, spawn_floor_poller,
        spawn_obstruction_poller, spawn_stop_button_poller,
    },
    logger::LogEvent,
    network::{spawn_peer_discovery, spawn_state_broadcast, startup_peer_sync},
    types::{
        direction::Direction, elevator::Behaviour, event::Event, orders::OrderType,
        systemstate::SystemState,
    },
};
use crossbeam_channel::{self as cbc, select};
use driver_rust::elevio::elev::Elevator;
mod assigner;
mod config;
mod fsm;
mod hardware;
mod logger;
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

    logger::log(LogEvent::Startup {
        node_id: &my_id,
        sim_port,
        bcast_port,
    });

    let elevator_driver =
        Elevator::init(&elevator_address, NUM_FLOORS as u8).expect("Error connecting to Elevator");
    let mut system_state = SystemState::new(&my_id);

    let (event_tx, event_rx) = cbc::unbounded::<Event>();
    let (state_to_broadcast_tx, state_to_broadcast_rx) = cbc::unbounded::<SystemState>();
    let (eager_broadcast_tx, eager_broadcast_rx) = cbc::unbounded::<SystemState>();
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
        eager_broadcast_rx,
        peer_state_tx,
    );

    logger::log(LogEvent::InitialState {
        state: &system_state,
    });

    // Prime the broadcast ticker before the sync window so peers receive our
    // heartbeats and can send us their state (including our backed-up cab orders).
    // Without this, all nodes are silent during the sync window and never discover
    // each other on a fresh cluster start.
    state_to_broadcast_tx.send(system_state.clone()).unwrap();

    startup_peer_sync(&peer_state_rx, &mut system_state);

    // Mark startup sync as complete. After this point, recover_from_backup will
    // no longer fire in merge_with — this elevator's own cab state is authoritative.
    system_state.mark_sync_complete();

    logger::log(LogEvent::PostSyncState {
        state: &system_state,
    });

    // Send the recovered (merged) state so peers learn our final post-sync view.
    state_to_broadcast_tx.send(system_state.clone()).unwrap();

    // The last goal floor assigned to this elevator. Kept across floor-reached events
    // so that the FSM can open the door upon arrival even if the assigner transiently
    // assigns the order to another elevator at the exact moment we arrive.
    let mut current_goal: Option<u8> = assigner::decide_next_order(&system_state);

    loop {
        select! {
            recv(peer_state_rx) -> msg => {
                if let Ok(fetched_state) = msg {
                    if system_state.merge_with(&fetched_state) {
                        logger::log(LogEvent::SystemStateReceived {
                            state: &system_state,
                        });
                        current_goal = assigner::decide_next_order(&system_state);

                        // Do not command the FSM while moving between floors — the
                        // elevator must reach the next floor sensor before we can
                        // safely stop or redirect it. The FloorReached event will
                        // re-evaluate current_goal when the elevator lands.
                        let between_floors = {
                            let my_state = system_state.get_my_state();
                            my_state.get_behavior() == Behaviour::Moving
                                && my_state.get_floor().is_none()
                        };

                        if !between_floors {
                            fsm::step(
                                &elevator_driver,
                                &mut system_state,
                                current_goal,
                                event_tx.clone(),
                                false
                            );
                        }

                        system_state.update_lights(&elevator_driver);
                        state_to_broadcast_tx.send(system_state.clone()).unwrap();
                    }
                }
            }

            recv(event_rx) -> event => {
                match event {
                    Ok(Event::FloorReached(floor)) => {
                        logger::log(LogEvent::FloorReached { floor });
                        system_state.arrive_at_floor(floor);
                        elevator_driver.floor_indicator(floor);

                        // FIXME: Move this elsewhere
                        let my_state = system_state.get_my_state();
                        if my_state.get_behavior() == Behaviour::Idle {
                            elevator_driver.motor_direction(Direction::Stop.into());
                        }

                        // Use the cached goal so the FSM opens the door upon arrival
                        // even if the assigner transiently reassigns this order to
                        // another elevator at the exact moment we reach the floor.
                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            false
                        );

                        // Refresh the goal after FSM has processed the arrival
                        // (the order may have been cleared by clear_order above).
                        current_goal = assigner::decide_next_order(&system_state);

                    },

                    Ok(Event::ButtonPressed(floor,order)) => {
                        system_state.add_order(floor, order);

                        // For hall orders, send an immediate high-redundancy broadcast
                        // to minimise the crash window during which no peer has seen the order.
                        match order {
                            OrderType::HallUp | OrderType::HallDown => {
                                eager_broadcast_tx.send(system_state.clone()).unwrap();
                            }
                            OrderType::Cab => {}
                        }

                        current_goal = assigner::decide_next_order(&system_state);

                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            true
                        );
                    },

                    Ok(Event::PeerUpdate(update)) => {
                        if let Some(id) = &update.new {
                            system_state.peer_new(id);
                        }

                        for id in &update.lost {
                            logger::log(LogEvent::ElevatorDead { id });
                            system_state.peer_lost(id);
                        }

                        current_goal = assigner::decide_next_order(&system_state);
                        fsm::step(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            false
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
                        } else if my_state.get_current_timer_id() == timer_id {
                            elevator_driver.door_light(false);
                            my_state.close_door();

                            current_goal = assigner::decide_next_order(&system_state);
                            fsm::step(
                                &elevator_driver,
                                &mut system_state,
                                current_goal,
                                event_tx.clone(),
                                false
                            );
                        } else {
                            logger::log(LogEvent::StaleTimer { timer_id });
                        }

                    },

                    Ok(Event::Obstructed(obstructed)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_obstruction(obstructed);

                        if obstructed {
                            fsm::step(
                                &elevator_driver,
                                &mut system_state,
                                None,
                                event_tx.clone(),
                                false
                            );

                        } else {
                            let hardware_floor = elevator_driver.floor_sensor();
                            if hardware_floor.is_some(){
                                // Ensure door is open and timer restarted if we become unobstructed while the door is still open
                                if let Some(new_id) = my_state.open_door() {
                                    elevator_driver.door_light(true);
                                    fsm::spawn_door_timer(new_id, event_tx.clone());
                                }
                            }
                    }
                    },

                    Ok(Event::EmergencyStop(is_stopped)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_emergency_stop(is_stopped);
                        elevator_driver.stop_button_light(is_stopped);

                        if is_stopped {
                            fsm::step(
                                &elevator_driver,
                                &mut system_state,
                                None,
                                event_tx.clone(),
                                false
                            );
                        } else {
                            current_goal = assigner::decide_next_order(&system_state);
                            fsm::step(
                                &elevator_driver,
                                &mut system_state,
                                current_goal,
                                event_tx.clone(),
                                false
                            );
                        }
                    }
                    Err(_) => eprintln!("Error in event loop"),
                }
                system_state.update_lights(&elevator_driver);
                logger::log(LogEvent::SystemStateLocal {
                    state: &system_state,
                });
                state_to_broadcast_tx.send(system_state.clone()).unwrap();
            }
        }
    }
}

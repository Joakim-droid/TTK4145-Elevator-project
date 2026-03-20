//! Entry point for the elevator node.
//! Initializes hardware, networking, and shared state, then runs the main event loop.

use crate::{
    config::{NUM_FLOORS, PEER_DISCOVERY_BROADCAST_PORT},
    hardware::{
        initialize_elevator_position, spawn_button_poller, spawn_floor_poller,
        spawn_obstruction_poller, spawn_stop_button_poller,
    },
    logger::LogEvent,
    network::{spawn_peer_discovery, spawn_state_broadcast, startup_peer_sync},
    types::{event::Event, orders::OrderType, systemstate::SystemState},
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
    let (my_id, elevatorserver_port, broadcast_port) = parser::parse();
    let elevator_address = format!("localhost:{}", elevatorserver_port);

    logger::log(LogEvent::Startup {
        node_id: &my_id,
        elevatorserver_port,
        broadcast_port,
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
    spawn_peer_discovery(
        my_id.clone(),
        event_tx.clone(),
        PEER_DISCOVERY_BROADCAST_PORT,
    );
    spawn_state_broadcast(
        my_id.clone(),
        broadcast_port,
        state_to_broadcast_rx,
        eager_broadcast_rx,
        peer_state_tx,
    );

    logger::log(LogEvent::InitialState {
        state: &system_state,
    });

    // Prime the broadcast ticker before sync so peers can discover each other and share state.
    state_to_broadcast_tx.send(system_state.clone()).unwrap();
    startup_peer_sync(&peer_state_rx, &mut system_state);
    system_state.mark_sync_complete();

    logger::log(LogEvent::PostSyncState {
        state: &system_state,
    });

    // Send the recovered post-sync state.
    state_to_broadcast_tx.send(system_state.clone()).unwrap();

    let mut current_goal: Option<u8> = assigner::decide_next_order(&system_state);

    loop {
        select! {
            recv(peer_state_rx) -> msg => {
                if let Ok(fetched_state) = msg
                    && system_state.merge_with(&fetched_state) {
                        logger::log(LogEvent::SystemStateReceived {
                            state: &system_state,
                        });
                        current_goal = assigner::decide_next_order(&system_state);

                        fsm::execute_state_transition(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            false
                        );

                        system_state.update_lights(&elevator_driver);
                        state_to_broadcast_tx.send(system_state.clone()).unwrap();
                    }
            }

            recv(event_rx) -> event => {
                match event {
                    Ok(Event::FloorReached(floor)) => {
                        logger::log(LogEvent::FloorReached { floor });
                        system_state.arrive_at_floor(floor);
                        elevator_driver.floor_indicator(floor);


                        // Use cached goal at arrival to avoid missing door-open if assignment briefly flips.
                        fsm::execute_state_transition(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            false
                        );

                        // Refresh the goal after FSM has processed
                        current_goal = assigner::decide_next_order(&system_state);

                    },

                    Ok(Event::ButtonPressed(floor,order)) => {
                        system_state.add_order(floor, order);

                        // Send hall order as fast as possible.
                        if matches!(order, OrderType::HallUp | OrderType::HallDown) {
                            eager_broadcast_tx.send(system_state.clone()).unwrap();
                        }

                        current_goal = assigner::decide_next_order(&system_state);

                        fsm::execute_state_transition(
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

                        fsm::execute_state_transition(
                            &elevator_driver,
                            &mut system_state,
                            current_goal,
                            event_tx.clone(),
                            false
                        );
                    },

                    Ok(Event::DoorOpenTimeOut(timer_id)) => {
                        let my_state = system_state.get_my_state();

                        if my_state.get_current_timer_id() != timer_id {

                            logger::log(LogEvent::StaleTimer { timer_id });

                        } else if my_state.is_obstructed() {

                            if let Some(new_id) = my_state.open_door() {
                                elevator_driver.door_light(true);
                                fsm::spawn_door_timer(new_id, event_tx.clone());
                            }

                        } else {
                            elevator_driver.door_light(false);
                            my_state.close_door();

                            current_goal = assigner::decide_next_order(&system_state);

                            fsm::execute_state_transition(
                                &elevator_driver,
                                &mut system_state,
                                current_goal,
                                event_tx.clone(),
                                false
                            );
                        }

                    },

                    Ok(Event::Obstructed(obstructed)) => {
                        let my_state = system_state.get_my_state();
                        my_state.set_obstruction(obstructed);

                        if obstructed {
                            fsm::execute_state_transition(
                                &elevator_driver,
                                &mut system_state,
                                None,
                                event_tx.clone(),
                                false
                            );

                        } else {
                            let hardware_floor = elevator_driver.floor_sensor();
                            if hardware_floor.is_some(){
                                // Restart door timer to ensure door stays open for full duration after obstruction cleared.
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

                        let goal = if is_stopped {
                            None
                        } else {
                            current_goal = assigner::decide_next_order(&system_state);
                            current_goal
                        };

                        fsm::execute_state_transition(
                            &elevator_driver,
                            &mut system_state,
                            goal,
                            event_tx.clone(),
                            false
                        );
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

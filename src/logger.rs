//! Test logging infrastructure
//!
//! This module provides structured logging for the test infrastructure using
//! a single `LogEvent` enum with feature-based behavior.


use crate::types::{direction::Direction, systemstate::SystemState};

#[cfg(feature = "test-logging")]
use std::io::Write;


pub enum LogEvent<'a> {

    // ALWAYS-ON EVENTS

    /// Node startup with connection details
    Startup {
        node_id: &'a str,
        elevatorserver_port: u16,
        broadcast_port: u16,
    },

    /// Initial system state after initialization
    InitialState { state: &'a SystemState },

    /// System state after 2s peer sync window
    PostSyncState { state: &'a SystemState },


    // TEST-ONLY EVENTS

    /// SystemState snapshot received from network merge
    SystemStateReceived { state: &'a SystemState },

    /// SystemState snapshot after local event processing
    SystemStateLocal { state: &'a SystemState },

    /// Elevator arrived at a floor (floor sensor triggered)
    FloorReached { floor: u8 },

    /// Door opened at a floor
    DoorOpened { floor: u8 },

    /// Order cleared at a floor
    OrderCleared { floor: u8 },

    /// Motor started moving toward goal from idle
    MotorStart {
        floor: u8,
        direction: Direction,
        goal: u8,
    },

    /// Motor reversed direction while moving (goal changed)
    MotorReverse {
        floor: u8,
        from: Direction,
        to: Direction,
        goal: u8,
    },

    /// Stale door timer event ignored (timer ID mismatch)
    StaleTimer { timer_id: u64 },

    /// Peer declared dead by peer discovery timeout
    ElevatorDead { id: &'a str },

    /// Homing progress (still searching for initial floor sensor)
    Homing,

    /// State broadcast thread started on given port
    StateBroadcastSpawned { port: u16 },

    /// Peer discovery thread started on given port
    PeerDiscoverySpawned { port: u16 },

    /// JSON input sent to hall_request_assigner binary
    AssignerInput { json: &'a str },

    /// Assigner returned a goal floor (or None if no orders)
    AssignerDecision { goal: Option<u8> },
}


/// Log an event. Always-on events print unconditionally; test-only events
/// are compiled out when the `test-logging` feature is disabled.
pub fn log(event: LogEvent) {
    match event {
        LogEvent::Startup {
            node_id,
            elevatorserver_port,
            broadcast_port,
        } => {
            println!(
                "Starting elevator node '{}' on elevator server port {} with broadcast port {}",
                node_id, elevatorserver_port, broadcast_port
            );
        }

        LogEvent::InitialState { state } => {
            println!("Initial state:");
            println!("{}", state);
        }

        LogEvent::PostSyncState { state } => {
            println!("Post-sync state:");
            println!("{}", state);
        }

        #[cfg(feature = "test-logging")]
        LogEvent::SystemStateReceived { state } => {
            println!("Received state from network");
            println!("{}", state);
            std::io::stdout().flush().unwrap();
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::SystemStateReceived { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::SystemStateLocal { state } => {
            println!("Local event handled");
            println!("{}", state);
            std::io::stdout().flush().unwrap();
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::SystemStateLocal { .. } => {}


        #[cfg(feature = "test-logging")]
        LogEvent::FloorReached { floor } => {
            println!("[EVENT] floor_reached floor={}", floor);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::FloorReached { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::DoorOpened { floor } => {
            println!("[EVENT] door_opened floor={}", floor);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::DoorOpened { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::OrderCleared { floor } => {
            println!("[EVENT] order_cleared floor={}", floor);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::OrderCleared { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::MotorStart {
            floor,
            direction,
            goal,
        } => {
            println!(
                "[EVENT] motor_start floor={} direction={:?} goal={}",
                floor, direction, goal
            );
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::MotorStart { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::MotorReverse {
            floor,
            from,
            to,
            goal,
        } => {
            println!(
                "[EVENT] motor_reverse floor={} from={:?} to={:?} goal={}",
                floor, from, to, goal
            );
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::MotorReverse { .. } => {}


        #[cfg(feature = "test-logging")]
        LogEvent::StaleTimer { timer_id } => {
            println!("Ignored stale timer event (ID: {})", timer_id);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::StaleTimer { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::ElevatorDead { id } => {
            println!("Elevator dead: {}", id);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::ElevatorDead { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::Homing => {
            println!("Still homing... floor sensor is None.");
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::Homing => {}

        #[cfg(feature = "test-logging")]
        LogEvent::StateBroadcastSpawned { port } => {
            println!("State broadcast spawned on port {}", port);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::StateBroadcastSpawned { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::PeerDiscoverySpawned { port } => {
            println!("Peer discovery spawned on port {}", port);
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::PeerDiscoverySpawned { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::AssignerInput { json } => {
            println!(
                "Assigner input (local state + peers, excluding dead nodes):\n{}",
                json
            );
        }
        #[cfg(not(feature = "test-logging"))]
        LogEvent::AssignerInput { .. } => {}

        #[cfg(feature = "test-logging")]
        LogEvent::AssignerDecision { goal } => match goal {
            Some(floor) => println!("Assigner decided goal floor: {}", floor),
            None => println!("Assigner decided goal floor: None"),
        },
        #[cfg(not(feature = "test-logging"))]
        LogEvent::AssignerDecision { .. } => {}
    }
}

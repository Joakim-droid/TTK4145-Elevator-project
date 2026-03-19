//! Event definitions for messages produced by hardware polling, timers, and network updates.

use crate::types::orders::OrderType;
use network_rust::udpnet;

pub enum Event {
    FloorReached(u8),
    ButtonPressed(u8, OrderType),
    PeerUpdate(udpnet::peers::PeerUpdate),
    DoorOpenTimeOut(u64),
    Obstructed(bool),
    EmergencyStop(bool),
}

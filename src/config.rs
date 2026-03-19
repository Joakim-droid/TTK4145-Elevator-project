//! Central configuration values for timing, floor count, and network ports.

use std::time::Duration;

pub const NUM_FLOORS: usize = 4;
pub const DOOR_OPEN_DURATION: Duration = Duration::from_secs(3);
pub const PEER_DISCOVERY_BCAST_PORT: u16 = 16658;
pub const SYSTEMSTATE_BROADCAST_PORT: u16 = 16659;
pub const PEER_DISCOVERY_INTERVAL: Duration = Duration::from_millis(100);
pub const PEER_DISCOVERY_TIMEOUT: Duration = Duration::from_millis(1500);
pub const STATE_BROADCAST_INTERVAL: Duration = Duration::from_millis(250);
pub const STATE_BROADCAST_REDUNDANCY: usize = 2;
pub const HALL_ORDER_BROADCAST_REDUNDANCY: usize = 6;

//! Order type definitions for hall and cab requests.

use core::fmt;
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    HallUp = 0,
    HallDown = 1,
    Cab = 2,
}

impl From<OrderType> for u8 {
    fn from(o: OrderType) -> u8 {
        o as u8
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::HallUp => write!(f, "HallUp"),
            OrderType::HallDown => write!(f, "HallDown"),
            OrderType::Cab => write!(f, "Cab"),
        }
    }
}

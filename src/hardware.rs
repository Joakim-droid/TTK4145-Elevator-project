use crossbeam_channel::Sender;
use driver_rust::elevio::elev::Elevator;
use std::{
    thread::{self, sleep},
    time::Duration,
};

use crate::types::{event::Event, orders::OrderType};

pub fn spawn_button_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();
    let num_floors = elevator_handler.num_floors;

    thread::spawn(move || {
        let mut prev_val: Option<(u8, OrderType)> = None;

        loop {
            for floor in 0..num_floors {
                for call in [OrderType::Cab, OrderType::HallDown, OrderType::HallUp] {
                    let pressed = elevator_handler.call_button(floor, call.into());

                    if !pressed || prev_val == Some((floor, call)) {
                        continue;
                    }

                    if channel_sender
                        .send(Event::ButtonPressed(floor, call))
                        .is_err()
                    {
                        // Close thread if channel is down
                        return;
                    }
                    prev_val = Some((floor, call));
                }
            }
            sleep(Duration::from_millis(20));
        }
    });
}

pub fn spawn_floor_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();

    thread::spawn(move || {
        let mut prev_val: Option<u8> = None;

        loop {
            let current_floor = elevator_handler.floor_sensor();

            match current_floor {
                Some(current_floor) => {
                    if current_floor == u8::MAX || Some(current_floor) == prev_val {
                        continue;
                    }

                    if channel_sender
                        .send(Event::FloorReached(current_floor))
                        .is_err()
                    {
                        // Close thread if channel is down
                        break;
                    }
                    prev_val = Some(current_floor);
                }
                None => {
                    // eprintln!("floor_sensor error");
                }
            }
            sleep(Duration::from_millis(20));
        }
    });
}

use crate::types::{
    direction::Direction, event::Event, orders::OrderType, systemstate::SystemState,
};
use crossbeam_channel::Sender;
use driver_rust::elevio::elev::Elevator;
use std::{
    thread::{self, sleep},
    time::Duration,
};

pub fn initialize_elevator_position(elevator: &Elevator, system_state: &mut SystemState) {
    if let Some(floor) = elevator.floor_sensor() {
        elevator.motor_direction(Direction::Stop.into());
        elevator.floor_indicator(floor);
        system_state.arrive_at_floor(floor);
        return;
    }

    elevator.motor_direction(Direction::Down.into());

    loop {
        if let Some(floor) = elevator.floor_sensor() {
            elevator.motor_direction(Direction::Stop.into());
            elevator.floor_indicator(floor);
            system_state.arrive_at_floor(floor);
            return;
        }
        sleep(Duration::from_millis(20));
    }
}

pub fn spawn_button_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();
    let num_floors = elevator_handler.num_floors;

    thread::spawn(move || {
        let mut prev_states = [[false; 3]; 4];

        let button_types = [OrderType::Cab, OrderType::HallDown, OrderType::HallUp];

        loop {
            for floor in 0..num_floors {
                for (btn_idx, &call) in button_types.iter().enumerate() {
                    let is_pressed = elevator_handler.call_button(floor, call.into());
                    let was_pressed = prev_states[floor as usize][btn_idx];

                    prev_states[floor as usize][btn_idx] = is_pressed;

                    if !is_pressed || was_pressed {
                        continue;
                    }

                    if channel_sender
                        .send(Event::ButtonPressed(floor, call))
                        .is_err()
                    {
                        // Close thread if channel is down
                        return;
                    }
                }
            }
            sleep(Duration::from_millis(20));
        }
    });
}

pub fn spawn_floor_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();

    thread::spawn(move || {
        let mut prev_floor: Option<u8> = None;

        loop {
            let current_floor = elevator_handler.floor_sensor();

            if current_floor != prev_floor {
                if let Some(floor) = current_floor
                    && floor != u8::MAX
                {
                    let res = channel_sender.send(Event::FloorReached(floor));
                    if res.is_err() {
                        // Close thread if channel is down
                        break;
                    }
                }

                prev_floor = current_floor;
            }

            sleep(Duration::from_millis(20));
        }
    });
}

// Function to poll obstruction sensor and sending the corresponding event when it changes.
pub fn spawn_obstruction_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();

    thread::spawn(move || {
        let mut prev_val: Option<bool> = None;

        loop {
            let obstructed = elevator_handler.obstruction();

            if prev_val == Some(obstructed) {
                sleep(Duration::from_millis(20));
                continue;
            }
            if channel_sender.send(Event::Obstructed(obstructed)).is_err() {
                break;
            }
            prev_val = Some(obstructed);
            sleep(Duration::from_millis(20));
        }
    });
}

pub fn spawn_stop_button_poller(elevator: &Elevator, channel_sender: Sender<Event>) {
    let elevator_handler = elevator.clone();

    thread::spawn(move || {
        let mut prev_val: Option<bool> = None;

        loop {
            let is_stop_pressed = elevator_handler.stop_button();

            if prev_val == Some(is_stop_pressed) {
                sleep(Duration::from_millis(20));
                continue;
            }
            if channel_sender
                .send(Event::EmergencyStop(is_stop_pressed))
                .is_err()
            {
                break;
            }
            prev_val = Some(is_stop_pressed);
            sleep(Duration::from_millis(20));
        }
    });
}

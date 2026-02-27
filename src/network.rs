use crate::types::{event::Event, systemstate::SystemState};
use crossbeam_channel::{self as cbc, Receiver, Sender};
use network_rust::udpnet::{self, bcast};
use std::{
    thread::{self, sleep},
    time::Duration,
};

pub fn spawn_state_broadcast(
    my_id: String,
    port: u16,
    state_rx: Receiver<SystemState>,
    peer_state_tx: Sender<SystemState>,
) {
    println!("State broadcast spawned on port {}", port);

    let state_rx_clone = state_rx.clone();
    thread::spawn(move || {
        bcast::tx(port, state_rx_clone).ok();
    });

    thread::spawn(move || {
        let (network_state_tx, network_state_rx) = cbc::unbounded::<SystemState>();
        thread::spawn(move || {
            bcast::rx(port, network_state_tx).ok();
        });

        loop {
            if let Ok(state) = network_state_rx.recv() {
                // Ignore our own state
                if state.get_my_id() != my_id {
                    peer_state_tx.send(state).ok();
                }
            }
        }
    });
}

pub fn spawn_peer_discovery(my_id: String, event_tx: Sender<Event>, port: u16) {
    println!("Peer discovery spawned on port {}", port);
    thread::spawn(move || {
        let (peer_tx, peer_rx) = cbc::unbounded::<udpnet::peers::PeerUpdate>();

        thread::spawn(move || {
            let enable_tx = cbc::never();
            udpnet::peers::tx(port, my_id, enable_tx).ok();
        });

        thread::spawn(move || {
            udpnet::peers::rx(port, peer_tx).ok();
        });

        loop {
            if let Ok(update) = peer_rx.recv()
                && event_tx.send(Event::PeerUpdate(update)).is_err()
            {
                break;
            }

            sleep(Duration::from_millis(200))
        }
    });
}

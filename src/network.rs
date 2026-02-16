use crate::types::{event::Event, systemstate::SystemState};
use crossbeam_channel::{self as cbc, Receiver, Sender};
use network_rust::udpnet;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    thread::{self, sleep},
    time::Duration,
};

pub fn spawn_send_thread(socket: UdpSocket, data_rx: Receiver<SystemState>, external_port: u16) {
    println!("Send thread spawned");

    thread::spawn(move || {
        let ip_addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        let external_socket_addr = SocketAddr::new(ip_addr, external_port);

        loop {
            if let Ok(update) = data_rx.recv() {
                let bytes = serde_json::to_vec(&update).unwrap();
                socket.send_to(&bytes, external_socket_addr).unwrap();
            }

            // sleep(Duration::from_millis(500));
        }
    });
}

pub fn spawn_recieve_thread(socket: UdpSocket, peer_data_tx: Sender<SystemState>) {
    println!("Recieve thread spawned");

    thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((amount, _source_address)) => {
                    let state_bytes = &buffer[..amount];

                    match serde_json::from_slice::<SystemState>(state_bytes) {
                        Ok(state) => {
                            peer_data_tx
                                .send(state)
                                .expect("Failed to send serialized state");
                        }
                        Err(e) => println!("{}", e),
                    }
                }
                Err(e) => {
                    eprintln!("Recieve error: {}", e);
                    break;
                }
            }
        }
    });
}

pub fn spawn_peer_discovery(my_id: String, event_tx: Sender<Event>) {
    thread::spawn(move || {
        let (peer_tx, peer_rx) = cbc::unbounded();

        thread::spawn(move || {
            let enable_tx = cbc::never();
            udpnet::peers::tx(16658, my_id, enable_tx).ok();
        });

        thread::spawn(move || {
            udpnet::peers::rx(16658, peer_tx).ok();
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

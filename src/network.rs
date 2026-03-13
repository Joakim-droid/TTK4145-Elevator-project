use crate::types::{event::Event, systemstate::SystemState};
use crossbeam_channel::{self as cbc, Receiver, Sender};
use network_rust::udpnet::peers::PeerUpdate;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    str,
    thread,
    time::{Duration, Instant},
};

fn new_tx_socket() -> std::io::Result<UdpSocket> {
    let sock = Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?;
    sock.set_broadcast(true)?;
    sock.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    sock.set_reuse_port(true)?;
    Ok(sock.into_udp_socket())
}

fn new_rx_socket(port: u16) -> std::io::Result<UdpSocket> {
    let sock = Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?;
    sock.set_broadcast(true)?;
    sock.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    sock.set_reuse_port(true)?;

    let local_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    sock.bind(&SockAddr::from(local_addr))?;
    Ok(sock.into_udp_socket())
}

pub fn spawn_state_broadcast(
    my_id: String,
    port: u16,
    state_rx: Receiver<SystemState>,
    peer_state_tx: Sender<SystemState>,
) {
    println!("State broadcast spawned on port {}", port);

    thread::spawn(move || {
        let sock =
            new_tx_socket().expect("Failed to create state broadcast tx socket");
        let remote_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, port);
        loop {
            let data = state_rx.recv().expect("State broadcast channel closed");
            let serialized = serde_json::to_vec(&data).expect("Failed to serialize state");
            if let Err(err) = sock.send_to(&serialized, remote_addr) {
                eprintln!("State broadcast send failed: {err}");
            }
        }
    });

    thread::spawn(move || {
        let sock =
            new_rx_socket(port).expect("Failed to bind state broadcast rx socket");
        let mut buf = [0; 4096];

        loop {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    match serde_json::from_slice::<SystemState>(&buf[..n]) {
                        Ok(state) => {
                            if state.get_my_id() != my_id {
                                peer_state_tx.send(state).ok();
                            }
                        }
                        Err(err) => {
                            eprintln!("Invalid state broadcast packet: {err}");
                        }
                    }
                }
                Err(err) => eprintln!("State broadcast receive failed: {err}"),
            }
        }
    });
}

pub fn spawn_peer_discovery(my_id: String, event_tx: Sender<Event>, port: u16) {
    println!("Peer discovery spawned on port {}", port);

    let tx_id = my_id.clone();
    thread::spawn(move || {
        let sock =
            new_tx_socket().expect("Failed to create peer discovery tx socket");
        let remote_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, port);
        let ticker = cbc::tick(Duration::from_millis(15));
        loop {
            if ticker.recv().is_err() {
                break;
            }
            if let Err(err) = sock.send_to(tx_id.as_bytes(), remote_addr) {
                eprintln!("Peer discovery send failed: {err}");
            }
        }
    });

    thread::spawn(move || {
        let timeout = Duration::from_millis(500);
        let sock =
            new_rx_socket(port).expect("Failed to bind peer discovery rx socket");
        sock.set_read_timeout(Some(timeout))
            .expect("Failed to configure peer discovery timeout");

        let mut last_seen: HashMap<String, Instant> = HashMap::new();
        let mut buf = [0; 1024];

        loop {
            let mut modified = false;
            let mut update = PeerUpdate {
                peers: Vec::new(),
                new: None,
                lost: Vec::new(),
            };
            let now = Instant::now();

            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    if let Ok(id) = str::from_utf8(&buf[..n]) {
                        update.new = if !last_seen.contains_key(id) {
                            modified = true;
                            Some(id.to_string())
                        } else {
                            None
                        };
                        last_seen.insert(id.to_string(), now);
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => {
                    eprintln!("Peer discovery receive failed: {err}");
                }
            }

            for (id, when) in &last_seen {
                if now.duration_since(*when) > timeout {
                    update.lost.push(id.to_string());
                    modified = true;
                }
            }

            for id in &update.lost {
                last_seen.remove(id);
            }

            if modified {
                update.peers = last_seen.keys().cloned().collect();
                update.peers.sort();
                update.lost.sort();

                if event_tx.send(Event::PeerUpdate(update)).is_err() {
                    break;
                }
            }
        }
    });
}

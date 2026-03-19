# TTK4145 Elevator Project

A distributed elevator control system developed for the course *TTK4145 - Real-time Programming* at NTNU. The project coordinates multiple elevator nodes over the network, assigns hall calls across peers, and keeps the system operational when nodes disconnect or restart.

## Overview

The system is built as a set of Rust modules around a shared `SystemState`. 

Each node polls the simulator hardware for button presses, floor arrivals, obstruction, and stop signals. It then exchanges state with peers over UDP and runs order assignment based on the agreed state. Then it executes movement and door actions through a finite state machine and shares its updated state with the other nodes.

## Features

- Distributed hall-order handling across multiple elevator nodes
- Local cab-order storage and recovery after node restart
- Peer discovery and dead-node detection
- Conflict resolution for shared hall calls using epoch counters


## System Design

### Event Flow

The runtime is centered around the event loop in `src/main.rs`:

1. `hardware.rs` generates local events from the simulator and initializes the elevator position.
2. `network.rs` receives and broadcasts peers and `SystemState` messages.
3. `assigner.rs` calls the external `hall_request_assigner` binary to decide which floor this node should serve next.
4. `fsm.rs` translates the chosen goal into motor and door commands.
5. `SystemState` is updated, lights are refreshed, and the new state is broadcast to peers.

### Fault Tolerance Strategy

- Each elevator state carries a monotonically increasing `seq` value so newer peer state can replace older state safely.
- Hall requests use `hall_epoch` counters to resolve conflicting updates across nodes.
- Lost peers are excluded from assignment, allowing surviving elevators to take over stranded hall orders.
- When a node comes back online, it can recover its own cab requests from the replicated backup state held by peers.

### Main Modules

- `main.rs`: Initializes the node, spawns workers, and runs the main loop  
- `hardware.rs`: Polls simulator hardware and sends events (buttons, floors, safety)  
- `network.rs`: Handles peer discovery and UDP broadcast of *SystemState*  
- `assigner.rs`: Serializes state and calls `hall_request_assigner`  
- `fsm.rs`: Controls motor direction, door timing, and state transitions  
- `parser.rs`: Parses CLI arguments and sets default parameters  
- `types/`: Defines shared domain types (events, directions, orders, elevator state, system state)

## Repository Layout

```text
.
├── Cargo.toml
├── README.md
├── execs/
│   ├── SimElevatorServer
│   └── hall_request_assigner
├── run_multi.sh
├── src/
│   ├── assigner.rs
│   ├── config.rs
│   ├── fsm.rs
│   ├── hardware.rs
│   ├── main.rs
│   ├── network.rs
│   ├── parser.rs
│   └── types/
└── tests/
    ├── elevator_test.py
    ├── log_verifier.py
    └── scenarios.json
```

## Requirements

Before running the project, make sure the following are available:

- Rust and Cargo
- The simulator binary `execs/SimElevatorServer`
- The order assignment binary `execs/hall_request_assigner`

For the tests, you will also need: 

- Python 3 for the automated test and log verification
- `tmux` for the chaos-test setup in `tests/elevator_test.py`


## Getting Started

### Build

```bash
cargo build
```

### Run a Single Node

Start a simulator instance in one terminal:

```bash
cd execs
./SimElevatorServer --port 15657
```

Then start one elevator node from the project root:

```bash
cargo run -- elev1 15657 16659
```

Command-line usage:

```text
cargo run -- [id] [sim_port] [bcast_port]
```

### Run a Three-Node Cluster

```bash
./run_multi.sh
```

This script:
kills any previously running simulators and nodes, builds the Rust project, launches three simulator instances on ports `15657`, `15658`, and `15659` and starts the nodes `elev1`, `elev2`, and `elev3` on broadcast port `16659`.

## Notes

- The project is currently configured for `4` floors in `src/config.rs`.
- `run_multi.sh` is the fastest way to demonstrate the distributed setup locally.


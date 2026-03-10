#!/usr/bin/env python3
import subprocess
import time
import sys
import os
import signal
import socket
import json
from log_verifier import run_verification

# Configuration
NODES = {"elev1": 15657, "elev2": 15658, "elev3": 15659}
BCAST_PORT = 16659
# ROOT_DIR should point to the project root so `execs/` is found there.
ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Keep the scenario config next to this test file.
TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(TESTS_DIR, "scenarios.json")

node_processes = {}
_current_scenario: dict = {}   # set before each scenario runs

# Simulator keyboard mappings (from SimElevatorServer docs)
_HALL_UP_KEYS   = "qwertyui"
_HALL_DOWN_KEYS = "sdfghjkl"
_CAB_KEYS       = "zxcvbnm,."

# Each simulator lives in its own pane inside window 0 of this session.
TMUX_SESSION = "elevator_chaos"

# Maps node_id -> pane index, filled by start_simulators().
_NODE_PANE: dict = {}


def _tmux_target(node_id):
    return f"{TMUX_SESSION}:0.{_NODE_PANE[node_id]}"

# ==========================================
# INFRASTRUCTURE (Setup / Teardown)
# ==========================================


def build_project():
    print("[*] Compiling Rust project...")
    subprocess.run(["cargo", "build"], check=True)


def wait_for_simulator(port, timeout=10):
    start = time.time()
    while time.time() - start < timeout:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.5)
            if sock.connect_ex(("127.0.0.1", port)) == 0:
                return True
        time.sleep(0.2)
    return False


def start_simulators():
    work_dir = os.path.join(ROOT_DIR, "execs")
    sim_bin = "./SimElevatorServer"
    if not os.path.exists(os.path.join(work_dir, "SimElevatorServer")) and \
            os.path.exists(os.path.join(work_dir, "SimElevatorServer.exe")):
        sim_bin = "./SimElevatorServer.exe"

    # Kill any leftover tmux session from a previous run
    subprocess.run(["tmux", "kill-session", "-t", TMUX_SESSION], capture_output=True)

    print(f"[*] Starting simulators in tmux session '{TMUX_SESSION}' (split-pane view)...")
    nodes = list(NODES.items())

    # Create the session; the first pane (index 0) holds the first simulator.
    first_id, first_port = nodes[0]
    _NODE_PANE[first_id] = 0
    subprocess.run([
        "tmux", "new-session", "-d", "-s", TMUX_SESSION, "-x", "240", "-y", "50",
    ], check=True)
    subprocess.run([
        "tmux", "send-keys", "-t", f"{TMUX_SESSION}:0.0",
        f"cd '{work_dir}' && {sim_bin} --port {first_port}", "Enter",
    ])

    # Add the remaining simulators as vertical splits.
    for i, (node_id, port) in enumerate(nodes[1:], 1):
        _NODE_PANE[node_id] = i
        subprocess.run(["tmux", "split-window", "-h", "-t", TMUX_SESSION + ":0"])
        subprocess.run([
            "tmux", "send-keys", "-t", f"{TMUX_SESSION}:0.{i}",
            f"cd '{work_dir}' && {sim_bin} --port {port}", "Enter",
        ])

    # Balance pane widths evenly.
    subprocess.run(["tmux", "select-layout", "-t", TMUX_SESSION + ":0", "even-horizontal"])

    print("[*] Waiting for simulators to accept connections...")
    for node_id, port in NODES.items():
        if wait_for_simulator(port):
            print(f"  [✓] Simulator for {node_id} on port {port} is ready!")
        else:
            print(f"  [!] Timeout waiting for simulator {node_id} ({port}). Exiting.")
            sys.exit(1)

    _open_tmux_viewer()


def _open_tmux_viewer():
    """Spawn a terminal window that attaches to the tmux session so the user
    can watch all three simulator panes side by side."""
    attach_cmd = f"tmux attach -t {TMUX_SESSION}"
    # All -e terminals accept:  terminal -e bash -c "cmd"
    # gnome-terminal uses -- instead of -e
    candidates = [
        ["gnome-terminal", "--", "bash", "-c", attach_cmd],
        ["x-terminal-emulator", "-e", "bash", "-c", attach_cmd],
        ["konsole", "-e", "bash", "-c", attach_cmd],
        ["xfce4-terminal", "-e", "bash", "-c", attach_cmd],
        ["xterm", "-T", "Elevator Simulators", "-e", "bash", "-c", attach_cmd],
    ]
    for cmd in candidates:
        try:
            subprocess.Popen(cmd)
            print(f"[*] Simulator view opened ({cmd[0]}). "
                  f"Also reachable via:  tmux attach -t {TMUX_SESSION}")
            return
        except FileNotFoundError:
            continue
    print(f"[!] Could not open a terminal automatically. "
          f"Run manually:  tmux attach -t {TMUX_SESSION}")


def start_node(node_id):
    if node_id in node_processes and node_processes[node_id][0].poll() is None:
        return
    port = NODES[node_id]
    logs_dir = os.path.join(TESTS_DIR, "logs")
    os.makedirs(logs_dir, exist_ok=True)
    log_path = os.path.join(logs_dir, f"{node_id}.log")
    print(f"  [+] Starting {node_id}  →  {log_path}")
    log_file = open(log_path, "w")
    p = subprocess.Popen(
        [
            os.path.join(ROOT_DIR, "target", "debug", "TTK4145-Elevator-project"),
            node_id,
            str(port),
            str(BCAST_PORT),
        ],
        stdout=log_file,
        stderr=subprocess.STDOUT,
        cwd=ROOT_DIR,
    )
    node_processes[node_id] = (p, log_file)


def kill_node(node_id):
    if node_id in node_processes:
        p, log_file = node_processes[node_id]
        if p.poll() is None:
            print(f"  [✗] KILL {node_id.upper()} — simulating hard crash")
            p.kill()
            p.wait()
        log_file.close()
        del node_processes[node_id]


def teardown(signum=None, frame=None):
    print("\n[*] Tearing down all processes...")
    for node_id in list(node_processes.keys()):
        kill_node(node_id)
    subprocess.run(["tmux", "kill-session", "-t", TMUX_SESSION], capture_output=True)
    os.system("pkill -f SimElevatorServer || true")
    os.system("pkill -f TTK4145-Elevator-project || true")
    sys.exit(0)


signal.signal(signal.SIGINT, teardown)

# ==========================================
# NETWORK INJECTION
# ==========================================


def get_current_network_state():
    """Sniff a single UDP broadcast packet from the cluster (used by wait_for_condition)."""
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        if hasattr(socket, "SO_REUSEPORT"):
            try:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
            except AttributeError:
                pass
        sock.bind(("", BCAST_PORT))
        sock.settimeout(0.5)
        try:
            while True:
                data, _ = sock.recvfrom(4096)
                state = json.loads(data.decode("utf-8"))
                if "hall_epoch" in state:
                    return state
        except (TimeoutError, json.JSONDecodeError):
            return None


def inject_order(order_type, floor, node_id=None):
    """Inject a button press via tmux send-keys into the target simulator window.
    This is identical to a human pressing a key inside the simulator terminal."""
    target = node_id or next(iter(NODES))

    if order_type == "hall_up":
        key = _HALL_UP_KEYS[floor]
    elif order_type == "hall_down":
        key = _HALL_DOWN_KEYS[floor]
    elif order_type == "cab":
        key = _CAB_KEYS[floor]
    else:
        print(f"  [!] Unknown order type: {order_type}")
        return

    print(f"  [kbd] '{key}' -> {target} simulator (floor {floor} {order_type})")
    # No Enter/C-m: the simulator reads raw keypresses, not newline-terminated commands.
    subprocess.run(["tmux", "send-keys", "-t", _tmux_target(target), key])


def wait_for_condition(condition_func, timeout=15):
    start = time.time()
    while time.time() - start < timeout:
        state = get_current_network_state()
        if state:
            res = condition_func(state)
            if res:
                return res
        time.sleep(0.1)
    return None


def execute_door_kill(floor, timeout):
    def check_door_open(state):
        for nid, nstate in state.get("states", {}).items():
            if nstate.get("floor") == floor and nstate.get("behaviour") == "doorOpen":
                return nid
        return None

    print(
        f"  [*] Waiting max {timeout}s for an elevator to open doors at Floor {floor}..."
    )
    serving_node = wait_for_condition(check_door_open, timeout=timeout)

    if serving_node:
        print(
            f"  [!] {serving_node.upper()} opened its doors! Striking it with lightning..."
        )
        kill_node(serving_node)
        print(
            "  [*] The order is now stranded. Another elevator must come rescue the passengers."
        )
    else:
        print("  [!] Scenario timed out. Elevators took too long.")


# ==========================================
# DYNAMIC CONFIG LOADER
# ==========================================


def run_scenarios_from_config():
    if not os.path.exists(CONFIG_FILE):
        print(f"[!] Configuration file not found: {CONFIG_FILE}")
        return

    with open(CONFIG_FILE, "r") as f:
        config = json.load(f)

    scenarios = config.get("scenarios", [])
    logs_dir  = os.path.join(TESTS_DIR, "logs")
    passed_all = True

    for idx, scenario in enumerate(scenarios, 1):
        _current_scenario.clear()
        _current_scenario.update(scenario)

        print("\n" + "═" * 64)
        print(f" [{idx}/{len(scenarios)}]  {scenario.get('name', 'Unnamed')}")
        if "description" in scenario:
            print(f"  {scenario['description']}")
        print("═" * 64)

        # Rotate logs: each scenario gets a fresh log file so the verifier
        # only sees state produced during this scenario.
        for node_id, (p, lf) in list(node_processes.items()):
            lf.close()
            new_lf = open(os.path.join(logs_dir, f"{node_id}.log"), "w")
            node_processes[node_id] = (p, new_lf)
            p.stdout = new_lf  # redirect already-running process (best effort)

        for step in scenario.get("steps", []):
            action = step.get("action")

            if action == "print":
                print(f"  [→] {step.get('message', '')}")

            elif action == "sleep":
                secs = step.get("duration", 1)
                print(f"  [⏳] Waiting {secs}s …")
                time.sleep(secs)

            elif action == "kill_node":
                kill_node(step.get("node_id"))

            elif action == "start_node":
                start_node(step.get("node_id"))

            elif action == "inject_order":
                inject_order(step.get("type"), step.get("floor"), step.get("node_id"))

            elif action == "wait_for_door_open":
                execute_door_kill(step.get("floor"), step.get("timeout", 15))

            elif action == "verify_logs":
                # Allow a moment for the last log lines to flush to disk.
                time.sleep(0.5)
                ok = run_verification(scenario, logs_dir, list(NODES.keys()))
                if not ok:
                    passed_all = False

            else:
                print(f"  [!] Unknown action: {action}")

    return passed_all


if __name__ == "__main__":
    os.system("pkill -f SimElevatorServer || true")
    os.system("pkill -f TTK4145-Elevator-project || true")

    build_project()
    start_simulators()

    print("\n[*] Starting Rust nodes...")
    for node_id in NODES.keys():
        start_node(node_id)

    print(
        "\n[✓] Cluster is stable. Starting Config-Driven Chaos Orchestrator in 3 seconds...\n"
    )
    time.sleep(3)

    run_scenarios_from_config()

    print("\n" + "═" * 64)
    print(" ALL AUTOMATED SCENARIOS COMPLETED")
    print("═" * 64)
    print("[*] Press Ctrl+C to stop everything and close the terminals.")
    while True:
        time.sleep(1)

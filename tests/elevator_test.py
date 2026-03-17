#!/usr/bin/env python3
import argparse
import subprocess
import time
import sys
import os
import signal
import socket
import json
import shutil
import platform
from log_verifier import run_verification

# Configuration
NODES = {"elev1": 15657, "elev2": 15658, "elev3": 15659}
PEER_DISCOVERY_PORT = 16658
BCAST_PORT = 16659
# ROOT_DIR should point to the project root so `execs/` is found there.
ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Keep the scenario config next to this test file.
TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(TESTS_DIR, "scenariosV2.json")

node_processes = {}
_current_scenario: dict = {}  # set before each scenario runs
CURRENT_LOG_DIR = os.path.join(TESTS_DIR, "logs")
NOISE_RULE_PATH = "/tmp/elevator_noise.rule"
NOISE_CONF_PATH = "/tmp/pf.elevator.conf"
NOISE_ENABLED = False
_NOISE_PROCESS = None
TEST_OPTIONS = {"noise": 0, "keep_running": False, "manual": False, "scenario": None}

# Simulator keyboard mappings (from SimElevatorServer docs)
_HALL_UP_KEYS = "qwertyui"
_HALL_DOWN_KEYS = "sdfghjkl"
_CAB_KEYS = "zxcvbnm,."
_OBSTRUCTION_KEY = "-"
_STOP_BUTTON_KEY = "p"

# Each simulator lives in its own pane inside window 0 of this session.
TMUX_SESSION = "elevator_chaos"

# Maps node_id -> pane index, filled by start_simulators().
_NODE_PANE: dict = {}


def _tmux_target(node_id):
    return f"{TMUX_SESSION}:0.{_NODE_PANE[node_id]}"


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--noise",
        type=int,
        default=0,
        help="Enable packet loss on UDP peer/state traffic during the test run (macOS pf).",
    )
    parser.add_argument(
        "--keep-running",
        action="store_true",
        help="Keep simulators and nodes alive after the suite finishes.",
    )
    parser.add_argument(
        "--manual",
        action="store_true",
        help=(
            "Start simulators and nodes without running any automated scenarios. "
            "Press Ctrl+C to stop."
        ),
    )
    parser.add_argument(
        "--scenario",
        type=str,
        default=None,
        help=(
            "Run only the specified scenario. "
            "Accepts a 1-based index (e.g. 4) or a substring of the scenario name (e.g. S4)."
        ),
    )
    return parser.parse_args()


def _record_issue(issues: list[str], message: str):
    if message not in issues:
        issues.append(message)


def _format_summary_lines(results: list[dict]) -> list[str]:
    total = len(results)
    passed = sum(1 for result in results if result["passed"])
    failed = total - passed

    lines = [
        "═" * 64,
        " TEST SUMMARY",
        "═" * 64,
        f" Passed: {passed}/{total}",
        f" Failed: {failed}/{total}",
    ]

    if TEST_OPTIONS["noise"]:
        lines.append(
            f" Noise: {TEST_OPTIONS['noise']}% packet loss on UDP {PEER_DISCOVERY_PORT}/{BCAST_PORT}"
        )
    lines.append("")

    for idx, result in enumerate(results, 1):
        status = "PASS" if result["passed"] else "FAIL"
        lines.append(f" [{idx:>2}] {status}  {result['name']}")
        for issue in result["issues"][:3]:
            lines.append(f"      - {issue}")
        if len(result["issues"]) > 3:
            lines.append(f"      - ... {len(result['issues']) - 3} more")
        lines.append("")

    return lines


def print_and_write_summary(results: list[dict], logs_dir: str):
    lines = _format_summary_lines(results)
    print("\n" + "\n".join(lines))
    os.makedirs(logs_dir, exist_ok=True)
    summary_path = os.path.join(logs_dir, "summary.txt")
    with open(summary_path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"[*] Wrote summary: {summary_path}")


def apply_noise(percent: int):
    global NOISE_ENABLED, _NOISE_PROCESS
    if percent <= 0:
        return

    system = platform.system()
    if system == "Darwin":
        rule = (
            "block drop in quick proto udp from any to any "
            f"port {{{PEER_DISCOVERY_PORT},{BCAST_PORT}}} probability {percent}%\n"
        )

        with open(NOISE_RULE_PATH, "w") as f:
            f.write(rule)

        with open("/etc/pf.conf", "r") as f:
            base_pf = f.read()

        with open(NOISE_CONF_PATH, "w") as f:
            f.write(base_pf)
            f.write('\nanchor "elevator_noise"\n')
            f.write(f'load anchor "elevator_noise" from "{NOISE_RULE_PATH}"\n')

        print(
            f"[*] Enabling {percent}% packet loss on UDP ports {PEER_DISCOVERY_PORT} and {BCAST_PORT}..."
        )
        subprocess.run(["sudo", "pfctl", "-f", NOISE_CONF_PATH], check=True)
        subprocess.run(["sudo", "pfctl", "-e"], check=True)
        subprocess.run(["sudo", "pfctl", "-a", "elevator_noise", "-sr"], check=True)
        NOISE_ENABLED = True
    elif system == "Linux":
        script_path = os.path.join(ROOT_DIR, "packetloss.sh")
        if not os.path.exists(script_path):
            raise RuntimeError(f"packetloss.sh not found at {script_path}")

        # Run packetloss.sh with sudo. Use -i (input) for discovery ports.
        # --unsafe prevents the safety timeout from stopping it early.
        cmd = [
            "sudo",
            script_path,
            str(percent),
            "-i",
            "--unsafe",
            str(PEER_DISCOVERY_PORT),
            str(BCAST_PORT),
        ]
        print(
            f"[*] Enabling {percent}% packet loss on UDP {PEER_DISCOVERY_PORT}/{BCAST_PORT} (Linux/iptables)..."
        )
        _NOISE_PROCESS = subprocess.Popen(cmd, start_new_session=True)
        NOISE_ENABLED = True
    else:
        raise RuntimeError(f"--noise is not supported on {system}")


def cleanup_noise():
    global NOISE_ENABLED, _NOISE_PROCESS
    if not NOISE_ENABLED:
        return

    system = platform.system()
    if system == "Darwin":
        print("[*] Disabling packet loss rules...")
        subprocess.run(["sudo", "pfctl", "-f", "/etc/pf.conf"], check=False)
        subprocess.run(["sudo", "pfctl", "-d"], check=False)
        for path in (NOISE_RULE_PATH, NOISE_CONF_PATH):
            try:
                os.remove(path)
            except FileNotFoundError:
                pass
    elif system == "Linux":
        if _NOISE_PROCESS and _NOISE_PROCESS.poll() is None:
            print("[*] Stopping packet loss script...")
            try:
                # Killing the process group sends the signal to both sudo and the bash script.
                # SIGINT triggers the trap cleanup in packetloss.sh.
                os.killpg(os.getpgid(_NOISE_PROCESS.pid), signal.SIGINT)
                _NOISE_PROCESS.wait(timeout=5)
            except Exception as e:
                print(f"[!] Error stopping noise process: {e}")
                # Manual fallback cleanup
                subprocess.run(["sudo", "iptables", "-F"], check=False)
        _NOISE_PROCESS = None

    NOISE_ENABLED = False


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
    if not os.path.exists(
        os.path.join(work_dir, "SimElevatorServer")
    ) and os.path.exists(os.path.join(work_dir, "SimElevatorServer.exe")):
        sim_bin = "./SimElevatorServer.exe"

    # Kill any leftover tmux session from a previous run
    subprocess.run(["tmux", "kill-session", "-t", TMUX_SESSION], capture_output=True)

    print(
        f"[*] Starting simulators in tmux session '{TMUX_SESSION}' (split-pane view)..."
    )
    nodes = list(NODES.items())

    # Create the session; the first pane (index 0) holds the first simulator.
    first_id, first_port = nodes[0]
    _NODE_PANE[first_id] = 0
    subprocess.run(
        [
            "tmux",
            "new-session",
            "-d",
            "-s",
            TMUX_SESSION,
            "-x",
            "240",
            "-y",
            "50",
        ],
        check=True,
    )
    subprocess.run(
        [
            "tmux",
            "send-keys",
            "-t",
            f"{TMUX_SESSION}:0.0",
            f"cd '{work_dir}' && {sim_bin} --port {first_port}",
            "Enter",
        ]
    )

    # Add the remaining simulators as vertical splits.
    for i, (node_id, port) in enumerate(nodes[1:], 1):
        _NODE_PANE[node_id] = i
        subprocess.run(["tmux", "split-window", "-h", "-t", TMUX_SESSION + ":0"])
        subprocess.run(
            [
                "tmux",
                "send-keys",
                "-t",
                f"{TMUX_SESSION}:0.{i}",
                f"cd '{work_dir}' && {sim_bin} --port {port}",
                "Enter",
            ]
        )

    # Balance pane widths evenly.
    subprocess.run(
        ["tmux", "select-layout", "-t", TMUX_SESSION + ":0", "even-horizontal"]
    )

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
    can watch all three simulator panes side by side in full screen."""
    attach_cmd = f"tmux attach -t {TMUX_SESSION}"
    # All -e terminals accept:  terminal -e bash -c "cmd"
    # gnome-terminal uses -- instead of -e
    candidates = [
        ["gnome-terminal", "--full-screen", "--", "bash", "-c", attach_cmd],
        ["x-terminal-emulator", "-e", "bash", "-c", attach_cmd],
        ["konsole", "--fullscreen", "-e", "bash", "-c", attach_cmd],
        ["xfce4-terminal", "--fullscreen", "-e", "bash", "-c", attach_cmd],
        [
            "xterm",
            "-maximized",
            "-T",
            "Elevator Simulators",
            "-e",
            "bash",
            "-c",
            attach_cmd,
        ],
    ]
    for cmd in candidates:
        try:
            subprocess.Popen(cmd)
            print(
                f"[*] Simulator view opened ({cmd[0]}). "
                f"Also reachable via:  tmux attach -t {TMUX_SESSION}"
            )
            return
        except FileNotFoundError:
            continue
    print(
        f"[!] Could not open a terminal automatically. "
        f"Run manually:  tmux attach -t {TMUX_SESSION}"
    )


def _node_launch_command(node_id, port):
    binary = os.path.join(ROOT_DIR, "target", "debug", "TTK4145-Elevator-project")
    base_cmd = [binary, node_id, str(port), str(BCAST_PORT)]

    if stdbuf := shutil.which("stdbuf"):
        return [stdbuf, "-oL", *base_cmd]
    if gstdbuf := shutil.which("gstdbuf"):
        return [gstdbuf, "-oL", *base_cmd]
    if script := shutil.which("script"):
        # macOS/BSD typically ship `script`, which gives us a PTY and line-flushed logs.
        return [script, "-q", "/dev/null", *base_cmd]
    return base_cmd


def start_node(node_id):
    if node_id in node_processes and node_processes[node_id][0].poll() is None:
        return
    port = NODES[node_id]
    os.makedirs(CURRENT_LOG_DIR, exist_ok=True)
    log_path = os.path.join(CURRENT_LOG_DIR, f"{node_id}.log")
    print(f"  [+] Starting {node_id}  →  {log_path}")
    # Use append mode so restarts within a scenario don't overwrite previous logs
    log_file = open(log_path, "a")
    launch_cmd = _node_launch_command(node_id, port)
    p = subprocess.Popen(
        launch_cmd,
        stdout=log_file,
        stderr=subprocess.STDOUT,
        cwd=ROOT_DIR,
        # Place the process in its own process group so kill_node can send
        # SIGKILL to the entire group (wrapper + its Rust child).
        start_new_session=True,
    )
    node_processes[node_id] = (p, log_file)


def kill_node(node_id):
    if node_id in node_processes:
        p, log_file = node_processes[node_id]
        if p.poll() is None:
            print(f"  [✗] KILL {node_id.upper()} — simulating hard crash")
            try:
                # Kill the entire process group: this takes out stdbuf AND the
                # Rust child process it spawned (plain p.kill() only kills stdbuf).
                os.killpg(os.getpgid(p.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                # Group already dead or PID recycled — fall back to direct kill.
                p.kill()
            p.wait()
        log_file.close()
        del node_processes[node_id]


def cleanup_all():
    print("\n[*] Tearing down all processes...")
    for node_id in list(node_processes.keys()):
        kill_node(node_id)
    subprocess.run(["tmux", "kill-session", "-t", TMUX_SESSION], capture_output=True)
    os.system("pkill -f SimElevatorServer || true")
    os.system("pkill -f TTK4145-Elevator-project || true")
    cleanup_noise()


def teardown(signum=None, frame=None):
    cleanup_all()
    sys.exit(0)


signal.signal(signal.SIGINT, teardown)

# ==========================================
# NETWORK INJECTION
# ==========================================


def get_current_network_state():
    """Collect UDP broadcast packets for up to 0.5 s and build a merged world-view.

    Strategy: accumulate every state entry seen from every packet.  When the
    same node appears in multiple packets, the reporter's *own* entry always
    wins (authoritative self-report), while third-party entries only fill gaps.
    This ensures that even if the first packet comes from a node that hasn't
    yet merged all peers, subsequent packets from those peers fill in the gaps.
    """
    # per_node_best[nid] = (seq, state_dict)
    # seq is used to prefer a node's own self-report over a peer's stale copy.
    per_node_best: dict = {}  # nid -> (is_self_report: bool, seq: int, state_dict)
    base_pkt: dict = {}  # used to carry hall_requests / hall_epoch etc.

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        if hasattr(socket, "SO_REUSEPORT"):
            try:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
            except AttributeError:
                pass
        sock.bind(("", BCAST_PORT))
        sock.settimeout(0.5)
        deadline = time.time() + 0.5
        while time.time() < deadline:
            try:
                data, _ = sock.recvfrom(4096)
            except (TimeoutError, OSError):
                break
            try:
                pkt = json.loads(data.decode("utf-8"))
            except (json.JSONDecodeError, UnicodeDecodeError):
                continue
            if "hall_epoch" not in pkt:
                continue
            reporter = pkt.get("my_id") or pkt.get("id")
            if not reporter:
                continue
            if not base_pkt:
                base_pkt = pkt
            # Merge every state entry from this packet into per_node_best.
            for nid, nstate in pkt.get("states", {}).items():
                is_self = nid == reporter
                seq = nstate.get("seq", 0)
                existing = per_node_best.get(nid)
                if existing is None:
                    per_node_best[nid] = (is_self, seq, nstate)
                else:
                    ex_self, ex_seq, _ = existing
                    # Self-report always beats a peer's copy; among same kind, higher seq wins.
                    if (is_self and not ex_self) or (
                        is_self == ex_self and seq > ex_seq
                    ):
                        per_node_best[nid] = (is_self, seq, nstate)

    if not base_pkt:
        return None

    merged = dict(base_pkt)
    merged["states"] = {nid: st for nid, (_, _, st) in per_node_best.items()}
    return merged


def inject_order(order_type, floor=None, node_id=None):
    """Inject a button press via tmux send-keys into the target simulator window.
    This is identical to a human pressing a key inside the simulator terminal."""
    target = node_id or next(iter(NODES))

    if order_type == "obstruction_toggle":
        key = _OBSTRUCTION_KEY
    elif order_type == "stop_button_toggle":
        key = _STOP_BUTTON_KEY
    elif floor is None:
        print(f"  [!] inject_order: 'floor' is required for order type '{order_type}'")
        return
    elif order_type == "hall_up":
        key = _HALL_UP_KEYS[floor]
    elif order_type == "hall_down":
        key = _HALL_DOWN_KEYS[floor]
    elif order_type == "cab":
        key = _CAB_KEYS[floor]
    else:
        print(f"  [!] Unknown order type: {order_type}")
        return

    label = f"floor {floor} " if floor is not None else ""
    print(f"  [kbd] '{key}' -> {target} simulator ({label}{order_type})")
    # No Enter/C-m: the simulator reads raw keypresses, not newline-terminated commands.
    subprocess.run(["tmux", "send-keys", "-t", _tmux_target(target), key])


def wait_for_condition(condition_func, timeout: float = 15):
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


def home_elevators():
    print("[*] Homing all elevators to Floor 0...")
    for node_id in NODES.keys():
        inject_order("cab", 0, node_id)

    # Wait until all elevators are at Floor 0 and Idle (or DoorOpen then Idle)
    def all_at_home(state):
        for nid in NODES.keys():
            nstate = state.get("states", {}).get(nid)
            if not nstate:
                return False
            if nstate.get("floor") != 0 or nstate.get("behaviour") not in [
                "idle",
                "doorOpen",
            ]:
                return False
        return True

    if wait_for_condition(all_at_home, timeout=25):
        print("[✓] All elevators homed to Floor 0.")
        # Extra wait to allow doors to close so they are strictly 'idle'
        time.sleep(4)
    else:
        print("[!] Homing failed or timed out. Continuing anyway...")


# ==========================================
# LIVE STATE VERIFICATION
# ==========================================


def _eval_condition(nstate: dict, key: str, val) -> bool:
    """Return True if nstate[key] matches val.  val may be a list of accepted values."""
    actual = nstate.get(key)
    if isinstance(val, list):
        return actual in val
    return actual == val


def wait_for_live_state(
    expected: dict,
    timeout: float = 20.0,
    require_all: bool = True,
    require_any: bool = False,
) -> bool:
    """Poll UDP broadcasts until nodes in *expected* show the desired state.

    Args:
        expected: { node_id: { key: value, ... }, ... }
                  Values may be a list of accepted alternatives, e.g. {"behaviour": ["idle","doorOpen"]}.
        timeout:    How long to poll in seconds.
        require_all: If True (default), ALL nodes in *expected* must match simultaneously.
                     If False, each node only needs to match at least once.
        require_any: If True, AT LEAST ONE node from *expected* must match at least once.
                     Overrides require_all when set to True.

    Returns True if the condition was satisfied within the timeout.
    """
    if require_any:
        # At least one node from the set must match at least once.
        start = time.time()
        while time.time() - start < timeout:
            state = get_current_network_state()
            if state:
                for nid, exp in expected.items():
                    nstate = state.get("states", {}).get(nid)
                    if nstate is None:
                        continue
                    if all(_eval_condition(nstate, k, v) for k, v in exp.items()):
                        return True
            time.sleep(0.1)
        return False
    elif require_all:

        def condition(state):
            for nid, exp in expected.items():
                nstate = state.get("states", {}).get(nid)
                if nstate is None:
                    return False
                for key, val in exp.items():
                    if not _eval_condition(nstate, key, val):
                        return False
            return True

        result = wait_for_condition(condition, timeout=timeout)
        return result is not None
    else:
        # Each node just needs to have matched at least once within the timeout.
        remaining = {nid: dict(exp) for nid, exp in expected.items()}
        satisfied: set = set()
        start = time.time()
        while time.time() - start < timeout:
            state = get_current_network_state()
            if state:
                for nid in list(remaining.keys()):
                    if nid in satisfied:
                        continue
                    nstate = state.get("states", {}).get(nid)
                    if nstate is None:
                        continue
                    if all(
                        _eval_condition(nstate, k, v) for k, v in remaining[nid].items()
                    ):
                        satisfied.add(nid)
                if satisfied >= set(remaining.keys()):
                    return True
            time.sleep(0.1)
        return False


def verify_live_state(
    expected: dict,
    timeout: float = 20.0,
    require_all: bool = True,
    require_any: bool = False,
    label: str = "",
) -> bool:
    """Check that nodes reach expected live state within *timeout* seconds.

    Unlike verify_state (log-based), this directly polls the UDP broadcast.
    Returns True on pass, False on fail, and always prints a clear result line.
    """
    tag = f" ({label})" if label else ""
    if require_any:
        mode = "any one node"
    elif require_all:
        mode = "all simultaneously"
    else:
        mode = "each at least once"
    print(f"  [live-check{tag}] Waiting up to {timeout}s for {mode}: {expected}")
    ok = wait_for_live_state(
        expected, timeout=timeout, require_all=require_all, require_any=require_any
    )
    if ok:
        print(f"  [✓] live-check{tag} PASSED")
    else:
        # Capture the last known state for diagnostics.
        last = get_current_network_state()
        print(f"  [✗] live-check{tag} FAILED — expected {expected}")
        if last:
            for nid, nstate in last.get("states", {}).items():
                if nid in expected:
                    print(
                        f"       {nid}: floor={nstate.get('floor')}  "
                        f"behaviour={nstate.get('behaviour')}  "
                        f"direction={nstate.get('direction')}"
                    )
    return ok


# ==========================================
# DYNAMIC CONFIG LOADER
# ==========================================


def run_scenarios_from_config():
    if not os.path.exists(CONFIG_FILE):
        print(f"[!] Configuration file not found: {CONFIG_FILE}")
        return False, []

    with open(CONFIG_FILE, "r") as f:
        config = json.load(f)

    scenarios = config.get("scenarios", [])

    # Filter by --scenario if specified
    scenario_filter = TEST_OPTIONS.get("scenario")
    if scenario_filter is not None:
        try:
            idx = int(scenario_filter) - 1
            if 0 <= idx < len(scenarios):
                scenarios = [scenarios[idx]]
            else:
                print(
                    f"[!] --scenario index {idx + 1} is out of range (1–{len(scenarios)})"
                )
                return False, []
        except ValueError:
            # Treat as a name substring match
            matched = [
                s
                for s in scenarios
                if scenario_filter.lower() in s.get("name", "").lower()
            ]
            if not matched:
                print(
                    f"[!] --scenario '{scenario_filter}' did not match any scenario name"
                )
                return False, []
            scenarios = matched
    logs_dir = os.path.join(TESTS_DIR, "logs")
    passed_all = True
    scenario_results: list[dict] = []

    # Start all nodes at the beginning of the test suite instead of per-scenario
    print("[*] Starting Rust nodes...")
    for node_id in NODES.keys():
        start_node(node_id)

    # Give nodes time to start broadcasting before the first scenario begins.
    # Without this, get_current_network_state() returns None and home_elevators()
    # polls the entire 25s window without seeing a single packet.
    print("[*] Waiting 8s for nodes to begin broadcasting...")
    time.sleep(8)

    print("\n[*] Starting Config-Driven Chaos Orchestrator...\n")

    for idx, scenario in enumerate(scenarios, 1):
        _current_scenario.clear()
        _current_scenario.update(scenario)

        print("\n" + "═" * 64)
        print(f" [{idx}/{len(scenarios)}]  {scenario.get('name', 'Unnamed')}")
        if "description" in scenario:
            print(f"  {scenario['description']}")
        print("═" * 64)

        # Home elevators before starting each scenario
        print("\n[✓] Cluster is stable. Homing elevators...")
        home_elevators()

        # Ensure elevators are somewhat stable before starting the scenario timer
        time.sleep(1)
        scenario_start_time = time.time()
        scenario_passed = True
        scenario_issues: list[str] = []

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

            elif action == "verify_state":
                # Legacy log-based state check (kept for backward compatibility).
                time.sleep(0.5)
                expected = step.get("expected", {})
                v_scenario = {
                    "verifications": [{"type": "current_state", "expected": expected}]
                }
                ok = run_verification(
                    v_scenario,
                    logs_dir,
                    list(NODES.keys()),
                    scenario_start_time,
                )
                if not ok:
                    passed_all = False
                    scenario_passed = False
                    _record_issue(scenario_issues, "state verification failed")

            elif action == "verify_state_live":
                # Live UDP-based state check — does not depend on log timing.
                expected = step.get("expected", {})
                timeout = step.get("timeout", 20.0)
                require_all = step.get("require_all", True)
                require_any = step.get("require_any", False)
                label = step.get("label", "")
                ok = verify_live_state(
                    expected,
                    timeout=timeout,
                    require_all=require_all,
                    require_any=require_any,
                    label=label,
                )
                if not ok:
                    passed_all = False
                    scenario_passed = False
                    issue = "live-check failed"
                    if label:
                        issue += f" ({label})"
                    _record_issue(scenario_issues, issue)

            elif action == "wait_for_live_state":
                # Block execution until the live state condition is met (no pass/fail).
                expected = step.get("expected", {})
                timeout = step.get("timeout", 20.0)
                require_all = step.get("require_all", True)
                require_any = step.get("require_any", False)
                reached = wait_for_live_state(
                    expected,
                    timeout=timeout,
                    require_all=require_all,
                    require_any=require_any,
                )
                if not reached:
                    print(f"  [!] Timed out waiting for state: {expected}")

            elif action == "verify_logs":
                time.sleep(0.5)
                ok = run_verification(
                    scenario, logs_dir, list(NODES.keys()), scenario_start_time
                )
                if not ok:
                    passed_all = False
                    scenario_passed = False
                    _record_issue(scenario_issues, "log verification failed")

            elif action == "set_noise":
                percent = step.get("percent", 0)
                print(f"  [~] Adjusting network noise to {percent}%...")
                if percent > 0:
                    apply_noise(percent)
                else:
                    cleanup_noise()

            elif action == "verify_logs_extended":
                # Extended log verification that includes per-node timeline dumps.
                time.sleep(0.5)
                extra_checks = step.get("checks", [])
                combined_scenario = dict(scenario)
                if extra_checks:
                    existing = combined_scenario.get("verifications", [])
                    combined_scenario["verifications"] = existing + extra_checks
                ok = run_verification(
                    combined_scenario,
                    logs_dir,
                    list(NODES.keys()),
                    scenario_start_time,
                    verbose=True,
                )
                if not ok:
                    passed_all = False
                    scenario_passed = False
                    _record_issue(scenario_issues, "extended log verification failed")

            else:
                print(f"  [!] Unknown action: {action}")
                passed_all = False
                scenario_passed = False
                _record_issue(scenario_issues, f"unknown action: {action}")

        status = "PASS" if scenario_passed else "FAIL"
        print(f"\n  Scenario result: {status}")
        # Always clean up noise at the end of a scenario to prevent leaking into the next
        cleanup_noise()
        scenario_results.append(
            {
                "name": scenario.get("name", "Unnamed"),
                "passed": scenario_passed,
                "issues": scenario_issues,
            }
        )

    return passed_all, scenario_results


if __name__ == "__main__":
    args = parse_args()
    TEST_OPTIONS["noise"] = max(0, min(100, args.noise))
    TEST_OPTIONS["keep_running"] = args.keep_running
    TEST_OPTIONS["manual"] = args.manual
    TEST_OPTIONS["scenario"] = args.scenario

    os.system("pkill -f SimElevatorServer || true")
    os.system("pkill -f TTK4145-Elevator-project || true")

    logs_dir = os.path.join(TESTS_DIR, "logs")
    passed_all = False
    scenario_results: list[dict] = []

    try:
        if os.path.exists(logs_dir):
            print("[*] Cleaning up old logs...")
            shutil.rmtree(logs_dir, ignore_errors=True)

        apply_noise(TEST_OPTIONS["noise"])
        build_project()
        start_simulators()

        if TEST_OPTIONS["manual"]:
            print("[*] Starting Rust nodes (manual mode — no scenarios will run)...")
            for node_id in NODES.keys():
                start_node(node_id)
            print("[*] All nodes started. Press Ctrl+C to stop everything.")
            while True:
                time.sleep(1)
        else:
            passed_all, scenario_results = run_scenarios_from_config()

            print("\n" + "═" * 64)
            print(" ALL AUTOMATED SCENARIOS COMPLETED")
            print("═" * 64)
            print_and_write_summary(scenario_results, logs_dir)
            if not passed_all:
                print("[!] One or more scenarios failed.")
            if TEST_OPTIONS["keep_running"]:
                print("[*] Press Ctrl+C to stop everything and close the terminals.")
                while True:
                    time.sleep(1)
    finally:
        cleanup_all()

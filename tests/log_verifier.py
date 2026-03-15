"""
log_verifier.py
~~~~~~~~~~~~~~~
Parse elevator node logs and verify correctness of scenario outcomes.

Log format (emitted by main.rs / systemstate.rs):
  Each "Received state from network" block looks like:

    Received state from network
    SystemState (id: elev1) @ 1773154588.316
    Hall requests:
      Floor  0: up=false (e=0), down=false (e=0)
      ...
    Elevators:
      elev1: ElevatorState { floor: Some(2), behaviour: Idle, ... seq: 19 }
      ...

We parse every such snapshot and build a timeline per node.
"""

from __future__ import annotations
import os
import re
from dataclasses import dataclass
from typing import Optional

NUM_FLOORS = 4

# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class ElevSnap:
    """One ElevatorState record parsed from a log line."""

    node_id: str
    floor: Optional[int]
    behaviour: str  # idle | moving | doorOpen
    direction: str  # Up | Down | Stop
    cab_requests: list[bool]
    seq: int


@dataclass
class StateSnap:
    """One full SystemState snapshot parsed from a node log."""

    reporter: str  # the node that wrote this log
    ts: float
    hall_requests: list[list[bool]]  # [floor][dir]  dir 0=Up 1=Down
    hall_epoch: list[list[int]]
    elevators: dict[str, ElevSnap]


# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------

_RE_HEADER = re.compile(r"SystemState \(id: (\S+)\) @ ([\d.]+)")
_RE_HALL = re.compile(
    r"Floor\s+(\d+): up=(true|false) \(e=(\d+)\), down=(true|false) \(e=(\d+)\)"
)
_RE_ELEV = re.compile(
    r"(\S+): ElevatorState \{"
    r".*?floor: (Some\((\d+)\)|None)"
    r".*?behaviour: (\w+)"
    r".*?direction: (\w+)"
    r".*?cab_requests: \[([^\]]+)\]"
    r".*?seq: (\d+)"
)
# Structured event markers emitted by fsm.rs / main.rs:
#   [EVENT] floor_reached floor=2
#   [EVENT] door_opened floor=2
#   [EVENT] order_cleared floor=2
#   [EVENT] motor_start floor=0 direction=Up goal=3
_RE_EVENT = re.compile(r"\[EVENT\] (\w+)(.*)")
_RE_KV = re.compile(r"(\w+)=(\S+)")


def parse_log(path: str, start_time: float = 0.0) -> list[StateSnap]:
    """Return all StateSnap objects found in *path*, in order."""
    if not os.path.exists(path):
        return []

    with open(path) as f:
        lines = f.readlines()

    snaps: list[StateSnap] = []
    last_snap_before = None
    i = 0
    while i < len(lines):
        m = _RE_HEADER.search(lines[i])
        if not m:
            i += 1
            continue

        reporter = m.group(1)
        ts = float(m.group(2))

        hall_req = [[False, False] for _ in range(NUM_FLOORS)]
        hall_epoch = [[0, 0] for _ in range(NUM_FLOORS)]
        elevators: dict[str, ElevSnap] = {}

        i += 1
        while i < len(lines):
            line = lines[i].rstrip()

            mh = _RE_HALL.search(line)
            if mh:
                fl = int(mh.group(1))
                if fl < NUM_FLOORS:
                    hall_req[fl][0] = mh.group(2) == "true"
                    hall_epoch[fl][0] = int(mh.group(3))
                    hall_req[fl][1] = mh.group(4) == "true"
                    hall_epoch[fl][1] = int(mh.group(5))
                i += 1
                continue

            me = _RE_ELEV.search(line)
            if me:
                nid = me.group(1)
                floor = int(me.group(3)) if me.group(2).startswith("Some") else None
                beh = me.group(4).lower()
                dire = me.group(5)
                cabs = [x.strip() == "true" for x in me.group(6).split(",")]
                seq = int(me.group(7))
                elevators[nid] = ElevSnap(nid, floor, beh, dire, cabs, seq)
                i += 1
                continue

            # A blank line or next header ends this block.
            if not line.strip() or _RE_HEADER.search(line):
                break
            i += 1

        snap = StateSnap(reporter, ts, hall_req, hall_epoch, elevators)
        if ts >= start_time:
            snaps.append(snap)
        else:
            last_snap_before = snap

    if last_snap_before:
        # Prepend the last known state before the scenario started
        snaps.insert(0, last_snap_before)

    return snaps


def load_all_logs(
    logs_dir: str, node_ids: list[str], start_time: float = 0.0
) -> dict[str, list[StateSnap]]:
    return {
        nid: parse_log(os.path.join(logs_dir, f"{nid}.log"), start_time)
        for nid in node_ids
    }


def parse_events(path: str) -> list[dict]:
    """Parse [EVENT] markers from a raw log file.

    Returns a list of dicts like:
      {"name": "floor_reached", "floor": 2}
      {"name": "door_opened", "floor": 2}
      {"name": "order_cleared", "floor": 2}
      {"name": "motor_start", "floor": 0, "direction": "Up", "goal": 3}
    """
    events: list[dict] = []
    if not os.path.exists(path):
        return events
    with open(path) as f:
        for line in f:
            m = _RE_EVENT.search(line)
            if not m:
                continue
            ev: dict = {"name": m.group(1)}
            for km in _RE_KV.finditer(m.group(2)):
                key, val = km.group(1), km.group(2)
                try:
                    ev[key] = int(val)
                except ValueError:
                    ev[key] = val
            events.append(ev)
    return events


def load_all_events(logs_dir: str, node_ids: list[str]) -> dict[str, list[dict]]:
    """Load [EVENT] markers for all nodes."""
    return {nid: parse_events(os.path.join(logs_dir, f"{nid}.log")) for nid in node_ids}


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------


class VerifyResult:
    def __init__(self, name: str = ""):
        self.name = name
        self.passed: list[str] = []
        self.failed: list[str] = []

    def ok(self, msg: str):
        self.passed.append(msg)

    def fail(self, msg: str):
        self.failed.append(msg)

    @property
    def success(self) -> bool:
        return len(self.failed) == 0


def _last_snap(logs: dict[str, list[StateSnap]]) -> dict[str, StateSnap]:
    """Most recent snapshot per reporting node."""
    return {nid: snaps[-1] for nid, snaps in logs.items() if snaps}


def check_hall_order_served(
    logs: dict[str, list[StateSnap]],
    floor: int,
    direction: int,  # 0=Up 1=Down
) -> VerifyResult:
    """
    Verify that a hall order (floor, direction) was eventually cleared.
    We look for a snapshot where hall_requests[floor][direction] == False
    AND the epoch is > 0 (meaning the order existed and was later cleared).
    """
    r = VerifyResult("hall_order_served")
    dir_name = "Up" if direction == 0 else "Down"
    label = f"hall {dir_name} floor {floor}"

    # Collect all snapshots across all nodes.
    all_snaps = [s for snaps in logs.values() for s in snaps]
    if not all_snaps:
        r.fail(f"{label}: no log data found")
        return r

    # Did the order ever appear?
    appeared = any(
        s.hall_requests[floor][direction]
        for s in all_snaps
        if floor < len(s.hall_requests)
    )

    # Did the order eventually clear (request=False, epoch>0)?
    cleared = any(
        not s.hall_requests[floor][direction] and s.hall_epoch[floor][direction] > 0
        for s in all_snaps
        if floor < len(s.hall_requests)
    )

    if not appeared and not cleared:
        r.fail(f"{label}: order never showed up in any log — injection may have failed")
    elif not cleared:
        r.fail(f"{label}: order was never cleared — failover DID NOT work")
    else:
        r.ok(f"{label}: order was served and cleared ✓")

    return r


def check_cab_order_served(
    logs: dict[str, list[StateSnap]],
    node_id: str,
    floor: int,
    events: Optional[list[dict]] = None,
) -> VerifyResult:
    """
    Verify that *node_id* eventually cleared its cab request for *floor*.
    We look for a snapshot reported by that node where its own cab_requests[floor] == False
    after having been True at some earlier point.

    Falls back to [EVENT] door_opened markers when the order is served so fast that
    the True state is never captured in a broadcast snapshot (e.g. elevator already
    at the target floor).
    """
    r = VerifyResult("cab_order_served")
    label = f"cab floor {floor} on {node_id}"

    node_snaps = logs.get(node_id, [])
    if not node_snaps:
        r.fail(f"{label}: no log file for {node_id}")
        return r

    my_snaps = [s.elevators[node_id] for s in node_snaps if node_id in s.elevators]

    if not my_snaps:
        r.fail(f"{label}: {node_id} never appeared in its own snapshots")
        return r

    # Find first snapshot where cab is True; cleared only counts as True AFTER that point.
    first_true_idx = next(
        (
            i
            for i, e in enumerate(my_snaps)
            if floor < len(e.cab_requests) and e.cab_requests[floor]
        ),
        None,
    )
    appeared = first_true_idx is not None
    cleared = appeared and any(
        not e.cab_requests[floor]
        for e in my_snaps[first_true_idx + 1 :]
        if floor < len(e.cab_requests)
    )

    if not appeared:
        # Fallback: order served so fast that cab_requests[floor]=True was never
        # captured in a snapshot (elevator already at floor 0, for example).
        # A [EVENT] door_opened floor=N marker is definitive proof of service.
        if events is not None and any(
            ev.get("name") == "door_opened" and ev.get("floor") == floor
            for ev in events
        ):
            r.ok(f"{label}: cab order served (confirmed via [EVENT] door_opened) ✓")
            return r
        r.fail(
            f"{label}: cab order never appeared in {node_id}'s log — injection or merge failed"
        )
    elif not cleared:
        r.fail(
            f"{label}: {node_id} registered the order but never cleared it — elevator did NOT serve it"
        )
    else:
        r.ok(f"{label}: cab order was served and cleared ✓")

    # Check resurrection: the cab request must have appeared AFTER a restart.
    # A restart is identified by seq going backwards or starting at a low value again.
    if appeared:
        seq_vals = [e.seq for e in my_snaps]
        restarts = [i for i in range(1, len(seq_vals)) if seq_vals[i] < seq_vals[i - 1]]
        if restarts:
            restart_idx = restarts[-1]
            post_restart = my_snaps[restart_idx:]
            post_first_true = next(
                (
                    i
                    for i, e in enumerate(post_restart)
                    if floor < len(e.cab_requests) and e.cab_requests[floor]
                ),
                None,
            )
            post_appeared = post_first_true is not None
            post_cleared = post_appeared and any(
                not e.cab_requests[floor]
                for e in post_restart[post_first_true + 1 :]
                if floor < len(e.cab_requests)
            )
            if post_appeared and post_cleared:
                r.ok(f"{label}: resurrected from peer state and served after restart ✓")
            elif post_appeared:
                r.fail(
                    f"{label}: cab merged after restart but NOT cleared — elevator did not serve it"
                )
            else:
                r.fail(
                    f"{label}: after restart, {node_id} never saw its cab order — peer-merge FAILED"
                )

    return r


def check_all_hall_orders_cleared(
    logs: dict[str, list[StateSnap]],
) -> VerifyResult:
    """Check that every hall order that was ever set is eventually cleared."""
    r = VerifyResult("all_hall_orders_cleared")
    all_snaps = [s for snaps in logs.values() for s in snaps]
    if not all_snaps:
        r.fail("no log data found")
        return r

    for fl in range(NUM_FLOORS):
        for di, dir_name in enumerate(["Up", "Down"]):
            epoch_ever_positive = any(
                s.hall_epoch[fl][di] > 0 for s in all_snaps if fl < len(s.hall_epoch)
            )
            if not epoch_ever_positive:
                continue  # This order was never placed; skip.

            cleared = any(
                not s.hall_requests[fl][di] and s.hall_epoch[fl][di] > 0
                for s in all_snaps
                if fl < len(s.hall_requests)
            )

            if not cleared:
                r.fail(f"hall {dir_name} floor {fl}: placed but NEVER cleared")
            else:
                r.ok(f"hall {dir_name} floor {fl}: cleared ✓")

    if not r.passed and not r.failed:
        r.ok("no hall orders were detected — nothing to verify")

    return r


def check_no_unexpected_panics(logs_dir: str, node_ids: list[str]) -> VerifyResult:
    """Scan raw log files for fatal panics that were NOT part of a planned kill."""
    r = VerifyResult("no_unexpected_panics")
    for nid in node_ids:
        path = os.path.join(logs_dir, f"{nid}.log")
        if not os.path.exists(path):
            continue
        with open(path) as f:
            content = f.read()
        panics = re.findall(r"Fatal panic.*", content)
        if panics:
            r.fail(f"{nid}: unexpected panic(s): {panics}")
        else:
            r.ok(f"{nid}: no unexpected panics ✓")
    return r


def check_state_broadcast_health(logs: dict[str, list[StateSnap]]) -> VerifyResult:
    """
    Each node should appear in the other nodes' state snapshots, confirming
    network broadcasting is working and merge is happening.
    """
    r = VerifyResult("state_broadcast_health")
    all_reporters = list(logs.keys())
    seen_by: dict[str, set[str]] = {nid: set() for nid in all_reporters}

    for reporter, snaps in logs.items():
        for snap in snaps:
            for other_nid in snap.elevators:
                if other_nid != reporter:
                    seen_by[reporter].add(other_nid)

    for reporter, seen in seen_by.items():
        # Skip nodes with very few snapshots — they were likely killed before discovering peers.
        if len(logs.get(reporter, [])) < 5:
            continue
        missing = set(all_reporters) - {reporter} - seen
        if missing:
            r.fail(f"{reporter} never saw state from: {', '.join(sorted(missing))}")
        else:
            r.ok(f"{reporter} saw all peers ✓")

    return r


def check_current_states(
    logs: dict[str, list[StateSnap]],
    expected: dict[str, dict],
) -> VerifyResult:
    """
    Verify the most recent state of specified elevators against expected values.
    'expected' is a dict: { node_id: { "floor": 0, "behaviour": "idle", ... } }
    """
    r = VerifyResult("current_state")
    last_snaps = _last_snap(logs)

    for nid, exp_state in expected.items():
        if nid not in last_snaps:
            r.fail(f"{nid}: no state found in any log")
            continue

        snap = last_snaps[nid]
        if nid not in snap.elevators:
            r.fail(f"{nid}: not found in its own reported snapshot")
            continue

        actual = snap.elevators[nid]
        for key, val in exp_state.items():
            actual_val = getattr(actual, key, None)
            if actual_val != val:
                r.fail(f"{nid}: expected {key}={val}, but got {actual_val}")
            else:
                r.ok(f"{nid}: {key}={val} ✓")
    return r


# ---------------------------------------------------------------------------
# Granular checks
# ---------------------------------------------------------------------------


def check_elevator_reached_floor(
    logs: dict[str, list[StateSnap]],
    node_id: str,
    floor: int,
    after_ts: float = 0.0,
    events: Optional[list[dict]] = None,
) -> VerifyResult:
    """
    Verify that *node_id* was observed at *floor* at some point during the
    scenario (i.e., after *after_ts*).

    Falls back to [EVENT] floor_reached markers when snapshot coverage is sparse.
    """
    r = VerifyResult("elevator_reached_floor")
    label = f"{node_id} at floor {floor}"
    node_snaps = logs.get(node_id, [])

    # Primary: check StateSnap timeline.
    reached = any(
        s.ts >= after_ts
        and s.elevators.get(node_id) is not None
        and s.elevators[node_id].floor == floor
        for s in node_snaps
    )

    if not reached and events is not None:
        # Fallback: [EVENT] floor_reached floor=N markers.
        reached = any(
            ev.get("name") == "floor_reached" and ev.get("floor") == floor
            for ev in events
        )
        if reached:
            r.ok(f"{label}: confirmed via [EVENT] floor_reached marker ✓")
            return r

    if reached:
        r.ok(f"{label}: confirmed in log ✓")
    else:
        r.fail(f"{label}: never observed at floor {floor} in {node_id}'s log")
    return r


def check_hall_order_served_within(
    logs: dict[str, list[StateSnap]],
    floor: int,
    direction: int,
    max_seconds: float,
) -> VerifyResult:
    """
    Verify that a hall order was *placed* and then *cleared* within
    *max_seconds* of when it was first seen as active.
    """
    r = VerifyResult("hall_order_served_within")
    dir_name = "Up" if direction == 0 else "Down"
    label = f"hall {dir_name} floor {floor} within {max_seconds}s"

    all_snaps = sorted(
        [s for snaps in logs.values() for s in snaps],
        key=lambda s: s.ts,
    )

    if not all_snaps:
        r.fail(f"{label}: no log data found")
        return r

    # Find first snapshot where the order is active (request == True).
    first_active_ts: Optional[float] = None
    for s in all_snaps:
        if floor < len(s.hall_requests) and s.hall_requests[floor][direction]:
            first_active_ts = s.ts
            break

    if first_active_ts is None:
        # Fallback: the order may have been served so fast that we never captured
        # a True snapshot.  Use the first snapshot where epoch > 0 as the anchor —
        # that snapshot already reflects a "was placed and cleared" state.
        for s in all_snaps:
            if floor < len(s.hall_epoch) and s.hall_epoch[floor][direction] > 0:
                first_active_ts = s.ts
                break

    if first_active_ts is None:
        r.fail(f"{label}: order never appeared in any log — injection may have failed")
        return r

    deadline = first_active_ts + max_seconds

    # Find first snapshot *at or after* activation where it is cleared.
    # "Cleared" means request=False and epoch>0 (order existed and was served).
    # The anchor snapshot itself counts if the order was already cleared there
    # (fast-serve case where we never saw request=True).
    cleared_ts: Optional[float] = None
    for s in all_snaps:
        if s.ts < first_active_ts:
            continue
        if (
            floor < len(s.hall_requests)
            and not s.hall_requests[floor][direction]
            and s.hall_epoch[floor][direction] > 0
        ):
            cleared_ts = s.ts
            break

    if cleared_ts is None:
        r.fail(f"{label}: order appeared but was NEVER cleared")
    elif cleared_ts > deadline:
        elapsed = cleared_ts - first_active_ts
        r.fail(
            f"{label}: cleared after {elapsed:.1f}s — exceeded {max_seconds}s deadline"
        )
    else:
        elapsed = cleared_ts - first_active_ts
        r.ok(f"{label}: cleared after {elapsed:.1f}s ✓")

    return r


def check_cab_order_served_within(
    logs: dict[str, list[StateSnap]],
    node_id: str,
    floor: int,
    max_seconds: float,
    events: Optional[list[dict]] = None,
) -> VerifyResult:
    """
    Verify that a cab order for *node_id* at *floor* was cleared within
    *max_seconds* of when it was first seen as active in that node's own log.

    Falls back to [EVENT] door_opened markers when the order is served so fast
    that the cab_requests[floor]=True state is never captured in a broadcast.
    """
    r = VerifyResult("cab_order_served_within")
    label = f"cab floor {floor} on {node_id} within {max_seconds}s"

    node_snaps = logs.get(node_id, [])
    if not node_snaps:
        r.fail(f"{label}: no log file for {node_id}")
        return r

    timed_snaps = [
        (s.ts, s.elevators[node_id]) for s in node_snaps if node_id in s.elevators
    ]

    first_active_ts: Optional[float] = None
    for ts, e in timed_snaps:
        if floor < len(e.cab_requests) and e.cab_requests[floor]:
            first_active_ts = ts
            break

    if first_active_ts is None:
        # Fallback: cab served before first broadcast captured cab_requests=True.
        # A [EVENT] door_opened floor=N from this node is proof of instant service.
        if events is not None and any(
            ev.get("name") == "door_opened" and ev.get("floor") == floor
            for ev in events
        ):
            r.ok(
                f"{label}: cab order served instantly (confirmed via [EVENT] door_opened) ✓"
            )
            return r
        r.fail(f"{label}: cab order never appeared in {node_id}'s log")
        return r

    deadline = first_active_ts + max_seconds
    cleared_ts: Optional[float] = None
    for ts, e in timed_snaps:
        if ts <= first_active_ts:
            continue
        if floor < len(e.cab_requests) and not e.cab_requests[floor]:
            cleared_ts = ts
            break

    if cleared_ts is None:
        r.fail(f"{label}: cab order appeared but was NEVER cleared")
    elif cleared_ts > deadline:
        elapsed = cleared_ts - first_active_ts
        r.fail(
            f"{label}: cleared after {elapsed:.1f}s — exceeded {max_seconds}s deadline"
        )
    else:
        elapsed = cleared_ts - first_active_ts
        r.ok(f"{label}: cab cleared after {elapsed:.1f}s ✓")

    return r


def check_no_order_stuck(
    logs: dict[str, list[StateSnap]],
    max_idle_seconds: float = 30.0,
) -> VerifyResult:
    """
    Check that no hall order remained active for longer than *max_idle_seconds*
    without being cleared, which would indicate a stuck/unserved order.
    """
    r = VerifyResult("no_order_stuck")
    all_snaps = sorted(
        [s for snaps in logs.values() for s in snaps],
        key=lambda s: s.ts,
    )

    if not all_snaps:
        r.ok("no log data — nothing to check")
        return r

    for fl in range(NUM_FLOORS):
        for di, dir_name in enumerate(["Up", "Down"]):
            first_active: Optional[float] = None
            last_active: Optional[float] = None
            was_cleared = False

            for s in all_snaps:
                if fl >= len(s.hall_requests):
                    continue
                active = s.hall_requests[fl][di]
                epoch = s.hall_epoch[fl][di]
                if active:
                    if first_active is None:
                        first_active = s.ts
                    last_active = s.ts
                elif epoch > 0 and first_active is not None:
                    was_cleared = True

            if first_active is None:
                continue  # Never placed.

            if not was_cleared:
                stuck_for = (last_active or first_active) - first_active
                if stuck_for > max_idle_seconds:
                    r.fail(
                        f"hall {dir_name} floor {fl}: stuck active for "
                        f"{stuck_for:.0f}s without being cleared"
                    )
                else:
                    # Still active but not for too long — could be in progress.
                    r.ok(f"hall {dir_name} floor {fl}: still active (not stuck yet)")
            else:
                r.ok(f"hall {dir_name} floor {fl}: not stuck ✓")

    if not r.passed and not r.failed:
        r.ok("no hall orders placed — nothing to check")

    return r


def check_behaviour_sequence(
    logs: dict[str, list[StateSnap]],
    node_id: str,
    expected_sequence: list[str],
) -> VerifyResult:
    """
    Verify that *node_id* passed through *expected_sequence* of behaviours
    in order at some point during the scenario.  Non-consecutive matches are
    allowed (i.e., other behaviours may appear in between).

    Example: expected_sequence=["moving", "doorOpen", "idle"] checks that
    the elevator moved, then opened its doors, then returned to idle.
    """
    r = VerifyResult("behaviour_sequence")
    label = f"{node_id} sequence {expected_sequence}"

    node_snaps = logs.get(node_id, [])
    if not node_snaps:
        r.fail(f"{label}: no log file for {node_id}")
        return r

    behaviours = [
        s.elevators[node_id].behaviour for s in node_snaps if node_id in s.elevators
    ]

    # Normalize expected sequence to lowercase to match the parsed log values
    # (the log parser lowercases behaviour strings via .lower()).
    normalized_seq = [b.lower() for b in expected_sequence]

    seq_idx = 0
    for beh in behaviours:
        if seq_idx < len(normalized_seq) and beh == normalized_seq[seq_idx]:
            seq_idx += 1
        if seq_idx == len(normalized_seq):
            break

    if seq_idx == len(normalized_seq):
        r.ok(f"{label}: full sequence observed ✓")
    else:
        matched = normalized_seq[:seq_idx]
        missing = normalized_seq[seq_idx:]
        r.fail(
            f"{label}: only matched {matched}, still waiting for {missing} "
            f"(observed behaviours: {list(dict.fromkeys(behaviours))})"
        )

    return r


# ---------------------------------------------------------------------------
# Timeline dump (verbose diagnostics)
# ---------------------------------------------------------------------------


def dump_timeline(
    logs: dict[str, list[StateSnap]],
    node_ids: list[str],
    start_time: float = 0.0,
) -> None:
    """Print a compact per-node timeline of behaviour + floor transitions."""
    print("\n  ── Timeline ─────────────────────────────────────────────")
    for nid in node_ids:
        snaps = logs.get(nid, [])
        if not snaps:
            print(f"  {nid}: (no snapshots)")
            continue

        transitions: list[str] = []
        prev_beh: Optional[str] = None
        prev_floor: Optional[int] = None

        for s in snaps:
            e = s.elevators.get(nid)
            if e is None:
                continue
            changed = e.behaviour != prev_beh or e.floor != prev_floor
            if changed:
                rel = f"+{s.ts - start_time:5.1f}s" if start_time else f"{s.ts:.1f}"
                transitions.append(f"{rel} fl={e.floor} {e.behaviour}/{e.direction}")
                prev_beh = e.behaviour
                prev_floor = e.floor

        print(f"  {nid}: " + "  →  ".join(transitions[:20]))
        if len(transitions) > 20:
            print(f"         ... ({len(transitions) - 20} more transitions)")

    print("  " + "─" * 56)


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------


def run_verification(
    scenario: dict,
    logs_dir: str,
    node_ids: list[str],
    start_time: float = 0.0,
    verbose: bool = False,
) -> bool:
    """
    Run the verification checks declared in *scenario* and print a report.
    Returns True if all checks pass.

    When *verbose* is True, a per-node state timeline is printed alongside
    the normal pass/fail lines, which makes debugging intermittent failures
    much easier.
    """
    logs = load_all_logs(logs_dir, node_ids, start_time)
    all_events = load_all_events(logs_dir, node_ids)
    results: list[VerifyResult] = []

    # Always-on checks
    results.append(check_no_unexpected_panics(logs_dir, node_ids))
    results.append(check_state_broadcast_health(logs))

    # Scenario-specific checks.
    raw_verifications = scenario.get("verifications")
    if raw_verifications is None:
        verify_type = scenario.get("verify")
        args = scenario.get("verify_args", {})
        raw_verifications = [{"type": verify_type, **args}] if verify_type else []

    for v in raw_verifications:
        vtype = v.get("type")
        if vtype == "hall_order_served":
            results.append(check_hall_order_served(logs, v["floor"], v["direction"]))
        elif vtype == "cab_order_served":
            nid = v["node_id"]
            results.append(
                check_cab_order_served(
                    logs, nid, v["floor"], events=all_events.get(nid)
                )
            )
        elif vtype == "all_hall_orders_cleared":
            results.append(check_all_hall_orders_cleared(logs))
        elif vtype == "current_state":
            results.append(check_current_states(logs, v["expected"]))
        elif vtype == "elevator_reached_floor":
            nid = v["node_id"]
            results.append(
                check_elevator_reached_floor(
                    logs,
                    nid,
                    v["floor"],
                    after_ts=v.get("after_ts", start_time),
                    events=all_events.get(nid),
                )
            )
        elif vtype == "hall_order_served_within":
            results.append(
                check_hall_order_served_within(
                    logs, v["floor"], v["direction"], v["max_seconds"]
                )
            )
        elif vtype == "cab_order_served_within":
            nid = v["node_id"]
            results.append(
                check_cab_order_served_within(
                    logs, nid, v["floor"], v["max_seconds"], events=all_events.get(nid)
                )
            )
        elif vtype == "no_order_stuck":
            results.append(
                check_no_order_stuck(
                    logs, max_idle_seconds=v.get("max_idle_seconds", 30.0)
                )
            )
        elif vtype == "behaviour_sequence":
            results.append(check_behaviour_sequence(logs, v["node_id"], v["sequence"]))
        else:
            print(f"  [!] Unknown verification type: {vtype}")

    # Print report
    all_pass = all(r.success for r in results)
    status_line = "PASS" if all_pass else "FAIL"
    width = 60

    print(f"\n  {'─' * width}")
    print(f"  VERIFICATION: {status_line}")
    print(f"  {'─' * width}")
    for r in results:
        if r.name:
            print(f"  [{r.name}]")
        for msg in r.passed:
            print(f"    [✓] {msg}")
        for msg in r.failed:
            print(f"    [✗] {msg}")
    print(f"  {'─' * width}")

    if verbose:
        dump_timeline(logs, node_ids, start_time)

    print()
    return all_pass

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
from dataclasses import dataclass, field
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
    behaviour: str          # idle | moving | doorOpen
    direction: str          # Up | Down | Stop
    cab_requests: list[bool]
    seq: int


@dataclass
class StateSnap:
    """One full SystemState snapshot parsed from a node log."""
    reporter: str           # the node that wrote this log
    ts: float
    hall_requests: list[list[bool]]   # [floor][dir]  dir 0=Up 1=Down
    hall_epoch: list[list[int]]
    elevators: dict[str, ElevSnap]


# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------

_RE_HEADER   = re.compile(r"SystemState \(id: (\S+)\) @ ([\d.]+)")
_RE_HALL     = re.compile(
    r"Floor\s+(\d+): up=(true|false) \(e=(\d+)\), down=(true|false) \(e=(\d+)\)")
_RE_ELEV     = re.compile(
    r"(\S+): ElevatorState \{"
    r".*?floor: (Some\((\d+)\)|None)"
    r".*?behaviour: (\w+)"
    r".*?direction: (\w+)"
    r".*?cab_requests: \[([^\]]+)\]"
    r".*?seq: (\d+)"
)


def parse_log(path: str) -> list[StateSnap]:
    """Return all StateSnap objects found in *path*, in order."""
    if not os.path.exists(path):
        return []

    with open(path) as f:
        lines = f.readlines()

    snaps: list[StateSnap] = []
    i = 0
    while i < len(lines):
        m = _RE_HEADER.search(lines[i])
        if not m:
            i += 1
            continue

        reporter = m.group(1)
        ts = float(m.group(2))
        hall_req  = [[False, False] for _ in range(NUM_FLOORS)]
        hall_epoch = [[0, 0] for _ in range(NUM_FLOORS)]
        elevators: dict[str, ElevSnap] = {}

        i += 1
        while i < len(lines):
            line = lines[i].rstrip()

            mh = _RE_HALL.search(line)
            if mh:
                fl = int(mh.group(1))
                if fl < NUM_FLOORS:
                    hall_req[fl][0]   = mh.group(2) == "true"
                    hall_epoch[fl][0] = int(mh.group(3))
                    hall_req[fl][1]   = mh.group(4) == "true"
                    hall_epoch[fl][1] = int(mh.group(5))
                i += 1
                continue

            me = _RE_ELEV.search(line)
            if me:
                nid   = me.group(1)
                floor = int(me.group(3)) if me.group(2).startswith("Some") else None
                beh   = me.group(4).lower()
                dire  = me.group(5)
                cabs  = [x.strip() == "true" for x in me.group(6).split(",")]
                seq   = int(me.group(7))
                elevators[nid] = ElevSnap(nid, floor, beh, dire, cabs, seq)
                i += 1
                continue

            # A blank line or next header ends this block.
            if not line.strip() or _RE_HEADER.search(line):
                break
            i += 1

        snaps.append(StateSnap(reporter, ts, hall_req, hall_epoch, elevators))

    return snaps


def load_all_logs(logs_dir: str, node_ids: list[str]) -> dict[str, list[StateSnap]]:
    return {nid: parse_log(os.path.join(logs_dir, f"{nid}.log")) for nid in node_ids}


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

class VerifyResult:
    def __init__(self):
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
    direction: int,   # 0=Up 1=Down
) -> VerifyResult:
    """
    Verify that a hall order (floor, direction) was eventually cleared.
    We look for a snapshot where hall_requests[floor][direction] == False
    AND the epoch is > 0 (meaning the order existed and was later cleared).
    """
    r = VerifyResult()
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
) -> VerifyResult:
    """
    Verify that *node_id* eventually cleared its cab request for *floor*.
    We look for a snapshot reported by that node where its own cab_requests[floor] == False
    after having been True at some earlier point.
    """
    r = VerifyResult()
    label = f"cab floor {floor} on {node_id}"

    node_snaps = logs.get(node_id, [])
    if not node_snaps:
        r.fail(f"{label}: no log file for {node_id}")
        return r

    my_snaps = [
        s.elevators[node_id]
        for s in node_snaps
        if node_id in s.elevators
    ]

    if not my_snaps:
        r.fail(f"{label}: {node_id} never appeared in its own snapshots")
        return r

    # Find first snapshot where cab is True; cleared only counts as True AFTER that point.
    first_true_idx = next(
        (i for i, e in enumerate(my_snaps) if floor < len(e.cab_requests) and e.cab_requests[floor]),
        None,
    )
    appeared = first_true_idx is not None
    cleared = appeared and any(
        not e.cab_requests[floor]
        for e in my_snaps[first_true_idx + 1:]
        if floor < len(e.cab_requests)
    )

    if not appeared:
        r.fail(f"{label}: cab order never appeared in {node_id}'s log — injection or merge failed")
    elif not cleared:
        r.fail(f"{label}: {node_id} registered the order but never cleared it — elevator did NOT serve it")
    else:
        r.ok(f"{label}: cab order was served and cleared ✓")

    # Check resurrection: the cab request must have appeared AFTER a restart.
    # A restart is identified by seq going backwards or starting at a low value again.
    if appeared:
        seq_vals = [e.seq for e in my_snaps]
        restarts = [i for i in range(1, len(seq_vals)) if seq_vals[i] < seq_vals[i-1]]
        if restarts:
            restart_idx = restarts[-1]
            post_restart = my_snaps[restart_idx:]
            post_first_true = next(
                (i for i, e in enumerate(post_restart)
                 if floor < len(e.cab_requests) and e.cab_requests[floor]),
                None,
            )
            post_appeared = post_first_true is not None
            post_cleared = post_appeared and any(
                not e.cab_requests[floor]
                for e in post_restart[post_first_true + 1:]
                if floor < len(e.cab_requests)
            )
            if post_appeared and post_cleared:
                r.ok(f"{label}: resurrected from peer state and served after restart ✓")
            elif post_appeared:
                r.fail(f"{label}: cab merged after restart but NOT cleared — elevator did not serve it")
            else:
                r.fail(f"{label}: after restart, {node_id} never saw its cab order — peer-merge FAILED")

    return r


def check_all_hall_orders_cleared(
    logs: dict[str, list[StateSnap]],
) -> VerifyResult:
    """Check that every hall order that was ever set is eventually cleared."""
    r = VerifyResult()
    all_snaps = [s for snaps in logs.values() for s in snaps]
    if not all_snaps:
        r.fail("no log data found")
        return r

    for fl in range(NUM_FLOORS):
        for di, dir_name in enumerate(["Up", "Down"]):
            epoch_ever_positive = any(
                s.hall_epoch[fl][di] > 0
                for s in all_snaps
                if fl < len(s.hall_epoch)
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
    r = VerifyResult()
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
    r = VerifyResult()
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


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------

def run_verification(
    scenario: dict,
    logs_dir: str,
    node_ids: list[str],
) -> bool:
    """
    Run the verification checks declared in *scenario* and print a report.
    Returns True if all checks pass.
    """
    logs = load_all_logs(logs_dir, node_ids)
    results: list[VerifyResult] = []

    # Always-on checks
    results.append(check_no_unexpected_panics(logs_dir, node_ids))
    results.append(check_state_broadcast_health(logs))

    # Scenario-specific checks.
    # Supports a "verifications" list (new style) or the legacy single "verify"/"verify_args".
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
            results.append(check_cab_order_served(logs, v["node_id"], v["floor"]))
        elif vtype == "all_hall_orders_cleared":
            results.append(check_all_hall_orders_cleared(logs))

    # Print report
    all_pass = all(r.success for r in results)
    status_line = "PASS" if all_pass else "FAIL"
    width = 60

    print(f"\n  {'─' * width}")
    print(f"  VERIFICATION: {status_line}")
    print(f"  {'─' * width}")
    for r in results:
        for msg in r.passed:
            print(f"  [✓] {msg}")
        for msg in r.failed:
            print(f"  [✗] {msg}")
    print(f"  {'─' * width}\n")

    return all_pass

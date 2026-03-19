#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Define the nodes: "ID elevatorserver_port"
NODES=(
  "elev1 15657"
  "elev2 15658"
  "elev3 15659"
)

# Shared broadcast port
broadcast_port=16659

wait_for_simulator() {
  local port="$1"
  local timeout="${2:-10}"
  local start_ts
  start_ts="$(date +%s)"

  while true; do
    if python3 - <<PY >/dev/null 2>&1
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.settimeout(0.2)
ok = s.connect_ex(("127.0.0.1", $port)) == 0
s.close()
raise SystemExit(0 if ok else 1)
PY
    then
      return 0
    fi

    if (( $(date +%s) - start_ts >= timeout )); then
      return 1
    fi

    sleep 0.2
  done
}

open_terminal() {
  local title="$1"
  local work_dir="$2"
  local cmd="$3"

  if [[ "${OSTYPE:-}" == darwin* ]]; then
    osascript <<EOF >/dev/null
tell application "Terminal"
    do script "cd '$work_dir'; $cmd"
    activate
end tell
EOF
  elif grep -qEi "(Microsoft|WSL)" /proc/version &>/dev/null; then
    powershell.exe -Command "Start-Process wsl -ArgumentList \"-d\", \"$WSL_DISTRO_NAME\", \"bash\", \"-c\", \"cd '$work_dir' && $cmd; exec bash\""
  elif [[ "${OSTYPE:-}" == msys* || "${OSTYPE:-}" == cygwin* ]]; then
    start bash -c "cd '$work_dir' && $cmd; exec bash"
  else
    if command -v x-terminal-emulator >/dev/null; then
      x-terminal-emulator -e bash -c "cd '$work_dir' && $cmd; exec bash" &
    elif command -v gnome-terminal >/dev/null; then
      gnome-terminal -- bash -c "cd '$work_dir' && $cmd; exec bash" &
    elif command -v kgx >/dev/null; then
      kgx -e bash -c "cd '$work_dir' && $cmd; exec bash" &
    elif command -v konsole >/dev/null; then
      konsole --title "$title" -e bash -c "cd '$work_dir' && $cmd; exec bash" &
    elif command -v xterm >/dev/null; then
      xterm -T "$title" -e bash -c "cd '$work_dir' && $cmd; exec bash" &
    else
      echo "No terminal emulator found. Running in background: $cmd"
      (cd "$work_dir" && $cmd) &
    fi
  fi
}

echo "Stopping any existing simulators or nodes..."
pkill -f SimElevatorServer || true
pkill -f TTK4145-Elevator-project || true

echo "Compiling Rust project..."
cargo build

echo "Starting simulators..."
for node in "${NODES[@]}"; do
  read -r id elevatorserver_port <<<"$node"
  
  SIM_CMD="./SimElevatorServer"
  # Support Windows .exe fallback
  if [[ ! -f "$ROOT_DIR/execs/SimElevatorServer" && -f "$ROOT_DIR/execs/SimElevatorServer.exe" ]]; then
    SIM_CMD="./SimElevatorServer.exe"
  fi
  
  open_terminal "Sim $elevatorserver_port" "$ROOT_DIR/execs" "$SIM_CMD --port $elevatorserver_port"
done

echo "Waiting for simulators to accept connections..."
for node in "${NODES[@]}"; do
  read -r id elevatorserver_port <<<"$node"
  if wait_for_simulator "$elevatorserver_port"; then
    echo "Simulator for $id on port $elevatorserver_port is ready"
  else
    echo "Timed out waiting for simulator $id on port $elevatorserver_port"
    exit 1
  fi
done

echo "Starting Rust nodes..."
for node in "${NODES[@]}"; do
  read -r id elevatorserver_port <<<"$node"
  echo "Launching $id on port $elevatorserver_port..."
  
  open_terminal "Node $id" "$ROOT_DIR" "./target/debug/TTK4145-Elevator-project $id $elevatorserver_port $broadcast_port"
done

echo "Done. All nodes launched."

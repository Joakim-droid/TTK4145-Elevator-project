#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIM_PATH="./execs/SimElevatorServer"


# Define the nodes: "ID SIM_PORT"
NODES=(
  "elev1 15657"
  "elev2 15658"
  "elev3 15659"
)

# Shared broadcast port
BCAST_PORT=16659

open_terminal() {
  local title="$1"
  local cmd="$2"

  if [[ "${OSTYPE:-}" == darwin* ]]; then
    # macOS
    osascript <<EOF >/dev/null
tell application "Terminal"
    do script "cd '$ROOT_DIR'; $cmd"
    activate
end tell
EOF
  elif grep -qEi "(Microsoft|WSL)" /proc/version &>/dev/null; then
    # WSL: Open a new Windows terminal running WSL
    # Use powershell to avoid the cmd.exe UNC path warning
    powershell.exe -Command "Start-Process wsl -ArgumentList \"-d\", \"$WSL_DISTRO_NAME\", \"bash\", \"-c\", \"cd '$ROOT_DIR' && $cmd; exec bash\""
  elif [[ "${OSTYPE:-}" == msys* || "${OSTYPE:-}" == cygwin* ]]; then
    # Windows (Git Bash / MSYS2)
    start bash -c "cd '$ROOT_DIR' && $cmd; exec bash"
  else
    # Linux (Generic)
    if command -v gnome-terminal >/dev/null; then
      gnome-terminal --title="$title" -- bash -c "cd '$ROOT_DIR' && $cmd; exec bash"
    elif command -v konsole >/dev/null; then
      konsole --title "$title" -e bash -c "cd '$ROOT_DIR' && $cmd; exec bash"
    elif command -v xterm >/dev/null; then
      xterm -T "$title" -e bash -c "cd '$ROOT_DIR' && $cmd; exec bash" &
    else
      echo "No terminal emulator found. Running in background: $cmd"
      (cd "$ROOT_DIR" && $cmd) &
    fi
  fi
}

echo "Stopping any existing simulators or nodes..."
# Use -f to match full command line and avoid 15-char limit
pkill -f SimElevatorServer || true
pkill -f TTK4145-Elevator-project || true

echo "Starting simulators..."
for node in "${NODES[@]}"; do
  read -r id sim_port <<<"$node"
  # Check if simulator exists as .exe or binary
  if [[ ! -f "$SIM_PATH" && -f "${SIM_PATH}.exe" ]]; then
    SIM_PATH="${SIM_PATH}.exe"
  fi
  
  open_terminal "Sim $sim_port" "$SIM_PATH --port $sim_port"
done

sleep 1

echo "Starting Rust nodes..."
for node in "${NODES[@]}"; do
  read -r id sim_port <<<"$node"
  echo "Launching $id on port $sim_port..."
  open_terminal "Node $id" "cargo run -- $id $sim_port $BCAST_PORT"
done

echo "Done. All nodes launched in separate windows."

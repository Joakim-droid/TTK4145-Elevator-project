#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"


# Format per rad: "SIM_PORT INTERNAL_PORT EXTERNAL_PORT"
NODES=(
  "15657 5001 5002"
  "15658 5002 5001"
)

open_terminal() {
  local cmd="$1"
  if [[ "${OSTYPE:-}" == darwin* ]]; then
    osascript <<EOF >/dev/null
tell application "Terminal"
    do script "cd '$ROOT_DIR'; $cmd"
    activate
end tell
EOF
  else
    echo "Kjør i ny terminal: cd '$ROOT_DIR'; $cmd"
  fi
}

echo "Starter simulatorer..."
for node in "${NODES[@]}"; do
  read -r sim_port _ _ <<<"$node"
  open_terminal "./Simulator-v2/SimElevatorServer --port $sim_port"
done

sleep 1

echo "Starter Rust-noder..."
for node in "${NODES[@]}"; do
  read -r sim_port internal_port external_port <<<"$node"
  open_terminal "cargo run -- $sim_port $internal_port $external_port"
done


#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
service_root="$(cd "${script_dir}/.." && pwd)"

# shellcheck source=./load_env_target.sh
source "${script_dir}/load_env_target.sh"

export GATEWAY_CONFIG_PATH="${GATEWAY_CONFIG_PATH:-${service_root}/gateway.local.toml}"
export GATEWAY_HOST="${GATEWAY_HOST:-0.0.0.0}"
export GATEWAY_PORT="${GATEWAY_PORT:-9100}"

cd "${service_root}"

echo "Starting DoWhiz local gateway"
echo "  gateway config: ${GATEWAY_CONFIG_PATH}"
echo "  gateway url:    http://localhost:${GATEWAY_PORT}"

cargo run -p scheduler_module --bin inbound_gateway

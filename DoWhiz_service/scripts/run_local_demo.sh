#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
service_root="$(cd "${script_dir}/.." && pwd)"

# shellcheck source=./load_env_target.sh
source "${script_dir}/load_env_target.sh"

export DEPLOY_TARGET="${DEPLOY_TARGET:-local}"
export EMPLOYEE_ID="${EMPLOYEE_ID:-sticky_octopus}"
export RUST_SERVICE_HOST="${RUST_SERVICE_HOST:-0.0.0.0}"
export RUST_SERVICE_PORT="${RUST_SERVICE_PORT:-9001}"

cd "${service_root}"

echo "Starting DoWhiz local demo service"
echo "  employee_id: ${EMPLOYEE_ID}"
echo "  frontend demo: http://localhost:5173/demo/workspace"
echo "  service demo:  http://localhost:${RUST_SERVICE_PORT}/browserbase-handoff-demo"

cargo run -p scheduler_module --bin rust_service -- \
  --host "${RUST_SERVICE_HOST}" \
  --port "${RUST_SERVICE_PORT}"

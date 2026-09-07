#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo run -q -p infra --bin test_accounts -- "$@"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "[Worker 4 Fix 3] Removing stale infra-dependent application test module..."

python3 - <<'PY'
from pathlib import Path

path = Path("crates/application/src/use_cases/charge_for_usage.rs")
text = path.read_text()

# The old unit-test module imported infra directly. PAYG integration tests
# now live under crates/infra/tests/, so application must remain independent
# of the infrastructure crate.
marker = "\n#[cfg(test)]\nmod tests {"

start = text.find(marker)

if start == -1:
    raise SystemExit(
        "No stale #[cfg(test)] mod tests block found in "
        "charge_for_usage.rs. Aborting without changes."
    )

# Safety check: only remove the test module if it contains the stale infra
# import. Never blindly truncate a source file.
test_module = text[start:]

if "use infra::" not in test_module:
    raise SystemExit(
        "Found a test module, but it does not contain the expected "
        "`use infra::` import. Aborting without changes."
    )

path.write_text(text[:start].rstrip() + "\n")
PY

echo "[Worker 4 Fix 3] Formatting..."
cargo fmt --all

echo "[Worker 4 Fix 3] Running application tests..."
cargo test -p application

echo "[Worker 4 Fix 3] Running PAYG integration tests..."
cargo test -p infra --test payg_charge_for_usage

echo "[Worker 4 Fix 3] Running all infra tests..."
cargo test -p infra

echo "[Worker 4 Fix 3] Workspace check..."
cargo check --workspace

echo "[Worker 4 Fix 3] PASS"

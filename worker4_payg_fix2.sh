#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "[Worker 4 Fix 2] Fixing PAYG integration-test wallet setup..."

python3 - <<'PY'
from pathlib import Path

path = Path("crates/infra/tests/payg_charge_for_usage.rs")
text = path.read_text()

# Every test credits the wallet directly. The in-memory repository requires
# get_or_create() first, matching the production wallet lifecycle.
needle = '''    wallet
        .credit('''

replacement = '''    wallet
        .get_or_create(tenant, "USD")
        .await
        .unwrap();

    wallet
        .credit('''

count = text.count(needle)

if count != 4:
    raise SystemExit(
        f"Expected exactly 4 wallet credit setup blocks, found {count}. "
        "Aborting without changes."
    )

text = text.replace(needle, replacement)

path.write_text(text)
PY

echo "[Worker 4 Fix 2] Formatting..."
cargo fmt --all

echo "[Worker 4 Fix 2] Running PAYG integration tests..."
cargo test -p infra --test payg_charge_for_usage

echo "[Worker 4 Fix 2] Running all infra tests..."
cargo test -p infra

echo "[Worker 4 Fix 2] Running application tests..."
cargo test -p application

echo "[Worker 4 Fix 2] Workspace check..."
cargo check --workspace

echo "[Worker 4 Fix 2] PASS"

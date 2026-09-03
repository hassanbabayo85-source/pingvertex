#!/usr/bin/env bash
set -euo pipefail

FILE="apps/leptos-dashboard/src/api/client.rs"

if [[ ! -f "$FILE" ]]; then
    echo "ERROR: $FILE not found"
    exit 1
fi

cp "$FILE" "${FILE}.bak_phase1_api_error_fix_v2"

python3 - "$FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

old = '''impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Unknown(format!("Invalid JSON response: {err}"))
    }
}
'''

new = '''impl From<serde_json::Error> for ApiError {
    fn from(_: serde_json::Error) -> Self {
        ApiError::Unknown(0, "Invalid JSON response.".to_string())
    }
}
'''

if old not in text:
    raise SystemExit(
        "ERROR: Expected invalid serde_json::Error conversion was not found."
    )

text = text.replace(old, new, 1)

path.write_text(text)
PY

cargo fmt --all

echo
echo "=== VERIFYING SERDE JSON ERROR CONVERSION ==="
grep -n -A5 -B2 \
  'impl From<serde_json::Error> for ApiError' \
  apps/leptos-dashboard/src/api/client.rs

echo
echo "=== FRONTEND CHECK ==="
cargo check -p leptos-dashboard --target wasm32-unknown-unknown

echo
echo "=== CLIPPY ==="
cargo clippy --workspace --all-targets -- -D warnings

echo
echo "=== WORKSPACE TESTS ==="
cargo test --workspace

echo
echo "=== FRONTEND RELEASE BUILD ==="
cd apps/leptos-dashboard
trunk build --release

echo
echo "=============================================="
echo "PHASE 1 API CLIENT FIX V2 PASSED"
echo "=============================================="

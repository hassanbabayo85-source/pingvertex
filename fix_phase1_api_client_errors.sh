#!/usr/bin/env bash
set -euo pipefail

FILE="apps/leptos-dashboard/src/api/client.rs"

if [[ ! -f "$FILE" ]]; then
    echo "ERROR: $FILE not found"
    exit 1
fi

cp "$FILE" "${FILE}.bak_phase1_api_error_fix"

python3 - "$FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

# ------------------------------------------------------------
# 1. Ensure serde_json::Error -> ApiError conversion exists.
# ------------------------------------------------------------
conversion = '''impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Unknown(format!("Invalid JSON response: {err}"))
    }
}

'''

if "impl From<serde_json::Error> for ApiError" not in text:
    marker = "impl From<gloo_net::Error> for ApiError {"
    if marker not in text:
        raise SystemExit(
            "ERROR: Could not locate gloo_net::Error conversion in client.rs"
        )
    text = text.replace(marker, conversion + marker, 1)

# ------------------------------------------------------------
# 2. gloo-net Response::headers().get() already returns Option.
# ------------------------------------------------------------
text = text.replace(
    'resp.headers().get("retry-after").ok().flatten()',
    'resp.headers().get("retry-after")'
)

# ------------------------------------------------------------
# 3. Some previous generated variants may have used the same
#    invalid pattern with different capitalization.
# ------------------------------------------------------------
text = text.replace(
    'resp.headers().get("Retry-After").ok().flatten()',
    'resp.headers().get("Retry-After")'
)

path.write_text(text)
PY

cargo fmt --all

echo
echo "=== API CLIENT ERROR FIX APPLIED ==="
echo
grep -n -A8 -B3 \
  'impl From<serde_json::Error> for ApiError' \
  apps/leptos-dashboard/src/api/client.rs || true

echo
echo "=== RETRY-AFTER REFERENCES ==="
grep -n 'retry-after\|Retry-After' \
  apps/leptos-dashboard/src/api/client.rs || true

echo
echo "=== RUNNING CLIPPY ==="
cargo clippy --workspace --all-targets -- -D warnings

echo
echo "=== RUNNING WORKSPACE TESTS ==="
cargo test --workspace

echo
echo "=== RUNNING FRONTEND BUILD ==="
cd apps/leptos-dashboard
trunk build --release

echo
echo "=============================================="
echo "PHASE 1 API CLIENT VERIFICATION PASSED"
echo "=============================================="

#!/usr/bin/env bash
set -euo pipefail

FILE="apps/leptos-dashboard/src/pages/status_pages.rs"

if [[ ! -f "$FILE" ]]; then
    echo "ERROR: $FILE not found"
    exit 1
fi

cp "$FILE" "${FILE}.bak_phase1_clippy_copy_callbacks"

python3 - "$FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

replacements = {
    "let save_callback = on_saved.clone();":
        "let save_callback = on_saved;",
    "let domain_callback = on_saved.clone();":
        "let domain_callback = on_saved;",
    "let remove_callback = on_saved.clone();":
        "let remove_callback = on_saved;",
    "let upload_callback = on_saved.clone();":
        "let upload_callback = on_saved;",
}

for old, new in replacements.items():
    if old not in text:
        raise SystemExit(f"ERROR: Expected line not found: {old}")
    text = text.replace(old, new, 1)

path.write_text(text)
PY

cargo fmt --all

echo
echo "=== VERIFY CALLBACK FIX ==="
grep -n -A2 -B2 \
  'save_callback\|domain_callback\|remove_callback\|upload_callback' \
  "$FILE"

echo
echo "=== RUNNING CLIPPY ==="
cargo clippy --workspace --all-targets -- -D warnings

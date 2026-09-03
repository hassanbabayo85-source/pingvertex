#!/usr/bin/env bash
# ============================================================
# apply-followup-fix.sh
#
# Follow-up patch (post-remediation review, 2026-09) closing ONE
# remaining item from the 12-issue audit remediation:
#
#   RUM `page_view_id` was still a plain FK on rum_js_errors and
#   rum_resource_errors (referencing rum_page_views(id) alone), instead
#   of a tenant/site composite relationship like every other RUM FK
#   fixed by migrations/0069. New migration 0072 closes that gap.
#   page_view_id is nullable, so this cannot affect any row where it's
#   already NULL — PostgreSQL's default MATCH SIMPLE FK semantics treat
#   a NULL in any referencing column as automatically satisfying the
#   constraint. See 0072's own header comment for full detail.
#
# This intentionally does NOT touch migration-0061's existing-data
# upgrade-path item — that one is a production-timeline/data decision,
# not a code fix, and is still open. Ask before requesting a script for
# that one; it will need real answers about your database's current
# state (approximate row counts in `checks`/`diagnostic_reports`,
# acceptable downtime/maintenance window) to write safely, since a
# wrong migration there risks live data.
#
# Same safety model as apply-final-fixes.sh: every modified file is
# checksum-verified against its known pre-image before being touched,
# backed up first, and this script is safe to re-run (it no-ops once
# already applied). Run this AFTER apply-final-fixes.sh (or against a
# repo already in that post-patch state) — HANDOVER.md's expected
# pre-image here is the version apply-final-fixes.sh itself produced,
# not the original.
#
# Usage:
#   cd /path/to/uptime-monitor
#   /path/to/apply-followup-fix.sh
# ============================================================
set -euo pipefail

REPO_ROOT="$(pwd)"

if [ ! -f "${REPO_ROOT}/Cargo.toml" ] || [ ! -d "${REPO_ROOT}/crates" ]; then
  echo "error: run this script from the uptime-monitor repo root (missing Cargo.toml / crates/)." >&2
  exit 1
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "${WORKDIR}"' EXIT

cat > "${WORKDIR}/patcher.py" << 'PATCHER_EOF'
#!/usr/bin/env python3
"""
Applies the PingVertex audit-remediation manifest.

For every MODIFIED file, this verifies the on-disk file's sha256 matches
the exact pre-image this patch was built against before touching
anything — if it doesn't match (repo already modified, wrong checkout,
patch already applied, etc.) that file is skipped with a clear message
instead of being blindly overwritten. Every file that IS modified is
backed up first (with a timestamped suffix) to a mirrored directory
under backups/<timestamp>/.

For every CREATED file, this refuses to overwrite an existing file at
that path (so re-running this script is safe / idempotent) and only
creates it if it does not already exist.

Usage: python3 patcher.py <manifest.json> <repo_root>
"""
import base64
import hashlib
import json
import os
import sys
import time


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: patcher.py <manifest.json> <repo_root>", file=sys.stderr)
        return 2

    manifest_path, repo_root = sys.argv[1], sys.argv[2]

    with open(manifest_path, "r", encoding="utf-8") as fh:
        manifest = json.load(fh)

    if not os.path.isfile(os.path.join(repo_root, "Cargo.toml")) or not os.path.isdir(
        os.path.join(repo_root, "crates")
    ):
        print(
            f"error: {repo_root} does not look like the uptime-monitor repo root "
            "(missing Cargo.toml and/or crates/) — aborting without touching anything.",
            file=sys.stderr,
        )
        return 1

    stamp = time.strftime("%Y%m%d-%H%M%S")
    backup_root = os.path.join(repo_root, "backups", f"audit-remediation-{stamp}")

    modified_ok, modified_skipped = [], []
    created_ok, created_skipped = [], []

    # ---- MODIFIED files: verify pre-image checksum, back up, overwrite ----
    for entry in manifest["modified"]:
        rel = entry["path"]
        target = os.path.join(repo_root, rel)
        expected_sha = entry["orig_sha256"]
        new_bytes = base64.b64decode(entry["new_b64"])

        if not os.path.isfile(target):
            print(f"SKIP  (missing)      {rel}")
            modified_skipped.append(rel)
            continue

        with open(target, "rb") as fh:
            current = fh.read()

        current_sha = hashlib.sha256(current).hexdigest()

        if current_sha == hashlib.sha256(new_bytes).hexdigest():
            print(f"SKIP  (already applied) {rel}")
            modified_ok.append(rel)  # already in the desired state; not an error
            continue

        if current_sha != expected_sha:
            print(
                f"SKIP  (checksum mismatch — file differs from the expected "
                f"pre-patch original; leaving it untouched) {rel}"
            )
            modified_skipped.append(rel)
            continue

        backup_path = os.path.join(backup_root, rel)
        os.makedirs(os.path.dirname(backup_path), exist_ok=True)
        with open(backup_path, "wb") as fh:
            fh.write(current)

        with open(target, "wb") as fh:
            fh.write(new_bytes)

        print(f"PATCH                {rel}")
        modified_ok.append(rel)

    # ---- CREATED files: only write if the path does not already exist ----
    for entry in manifest["created"]:
        rel = entry["path"]
        target = os.path.join(repo_root, rel)
        new_bytes = base64.b64decode(entry["new_b64"])

        if os.path.exists(target):
            with open(target, "rb") as fh:
                current = fh.read()
            if hashlib.sha256(current).hexdigest() == hashlib.sha256(new_bytes).hexdigest():
                print(f"SKIP  (already applied) {rel}")
                created_ok.append(rel)
            else:
                print(f"SKIP  (already exists, contents differ — not overwritten) {rel}")
                created_skipped.append(rel)
            continue

        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, "wb") as fh:
            fh.write(new_bytes)
        os.chmod(target, 0o755) if rel.endswith(".sh") else None
        print(f"CREATE               {rel}")
        created_ok.append(rel)

    print()
    print(f"Modified: {len(modified_ok)} applied/already-applied, {len(modified_skipped)} skipped")
    print(f"Created:  {len(created_ok)} applied/already-applied, {len(created_skipped)} skipped")
    if modified_ok or created_ok:
        print(f"Backups (of files actually overwritten) are under: {backup_root}")

    if modified_skipped or created_skipped:
        print()
        print(
            "One or more files were skipped because their current content did not "
            "match what this patch expects. This usually means the repo checkout "
            "is not the exact source this patch was generated from, or the file "
            "was already hand-edited. Review those files manually before re-running.",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
PATCHER_EOF

cat > "${WORKDIR}/manifest.json.gz.b64" << 'MANIFEST_EOF'
H4sICA75mGoAA21hbmlmZXN0Mi5qc29uALVc23Kj2JL9lRP9PHEGkFVtJmIeJCzQFZdAIGBiokOA
jWRAokp3nTj/PitzbyQkV3X3mZl+cNhlC9g798qVKy/UP34pN+nqffWW/vIff/uvf/xSLXZL/PRL
v2O/vPo95+9l+su//e2XzfdV9tt2udDaX+ivyfP7u7b4NdZ0XWm335/1dzV5UpVEfY9bz29PsZ6m
b+3n5zjGb9X3ty/v6pf2r0/t5/bzQls8qwu65frt+Fv85YluNzh3p4t5e+2X/jm1/KdB71QkfeeQ
nLtWUup7/G4fGV17MU/PYeBUsfaUPW0Mb9BbLuPSOaS4zsg2H7hPELeG36Ng8GXQO7YHr6NOYXRf
XFf+vjwd0nO3jFuDbGb5x9TSL4NedI61thLN27m4xyTzNPuQWN5o7Ha7kTXNpkqepZp+XmjRNm4l
WVTq58Yay9saO5sw6B6Ts76Ngq4Stya7yDIvC2t4CINhPnKf6J5mvLbVpPSyaD08xGuniMtpFq79
Kracy8Ayaa9KFNgK7n1O508Z7TcKoiJZ51nS9y8LQ596K75XLz5393GLrh8W4XyLzy2rpAUblUkW
lubHQmPbwQb8eSvWTmIPjc8llqlg/ed4PRFrcrsD+6wPJrPOp3sYqwnZeZbSNcFkNFV02+udTL/n
yTOxh4N+ekjKXRlbegv7ySPLz/Ede+tlieYfYYtlamH/ZbGNZtssLp/v1rMo9RX2vos1p8Jnisi4
PtdazGH3c/clKX3cw8/HinqIrKJczOln2NLycZZT7LXzbZTb52hu0r+/jVbdbFymVWot1XClt654
aE0OscTCeH7Dwnhd7GKrI/ZktQvgpvWIocb57Y2Vko1WlZ9YDq9rtNpkYW/ZneXOVz/33XGp5qHx
ssqG0wxrqAg3tJdxthkBcx9NHMKGmxD+EMG+OLNX/L4YWMUeNjwm1qkKNVNZzHX828bevV8Hlr6P
+znjFH4EPJqXtD9Uw5ZDONvXuPYtH7+fjGaW/hHOjxn5zWIe3d0vnqvkU5eBsVSArVXkCgwu8MUY
mRd7XFu9ZpvdwOp+ABfLpN+9jLXaH7DPAOsv9WUCXA9XYUZricutxPg0G7sqzuVU4HNF2BH3ASbW
Mdk14L8vY+uI7wr9/LGwfGCwQ7bIsNZzpD3j+6nCc9sDQ9mxb7bsYzi3C/p3OD9tx4GD56cF9ivu
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
AA==
MANIFEST_EOF

echo "==> Decoding manifest"
base64 -d "${WORKDIR}/manifest.json.gz.b64" > "${WORKDIR}/manifest.json.gz"
gunzip -f "${WORKDIR}/manifest.json.gz"

echo "==> Applying follow-up patch"
if ! python3 "${WORKDIR}/patcher.py" "${WORKDIR}/manifest.json" "${REPO_ROOT}"; then
  echo "==> Skipped one or more files — see messages above." >&2
  exit 1
fi

echo
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found — install Rust, then run cargo fmt/clippy/test yourself." >&2
  exit 0
fi

echo "==> cargo fmt --all"
cargo fmt --all
echo "==> cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings
echo "==> cargo test --workspace"
cargo test --workspace

echo
echo "==> Done. Apply migration 0072 against staging before production,"
echo "    same caution as 0069 — it will fail loudly if any existing"
echo "    non-NULL page_view_id row is already tenant/site-inconsistent."

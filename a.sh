#!/usr/bin/env bash
# ============================================================
# apply-checks-upgrade-tool.sh
#
# Adds ONE new operator tool — it does NOT touch your schema, and does
# NOT run any migration:
#
#   scripts/migrate_existing_checks_to_partitioned.sh
#
# A safe, manually-invoked, resumable, batched shadow-table copy + swap
# that converts an existing, POPULATED `checks` table into the exact
# partitioned structure migration 0061 creates — for databases where
# 0061 currently refuses to run because real data already exists.
#
# THIS SCRIPT (apply-checks-upgrade-tool.sh) only copies that one tool
# into your repo and updates HANDOVER.md's notes about it. It does not
# execute any data migration itself. Read
# scripts/migrate_existing_checks_to_partitioned.sh's own header comment
# in full before running IT — it defaults to a dry-run/plan-only mode
# and requires explicit --copy and --yes flags to actually touch data,
# but converting a live table's storage layout is inherently a serious
# operation and deserves a careful read first, ideally against a
# staging copy of your real data before production.
#
# Same safety model as the other apply-*.sh scripts here: the modified
# file (HANDOVER.md) is checksum-verified against its known pre-image
# before being touched and backed up first; the new file is never
# overwritten if something already exists at that path. Safe to re-run.
#
# Usage:
#   cd /path/to/uptime-monitor
#   /path/to/apply-checks-upgrade-tool.sh
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
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
yPNW//u3//z3//y/taz3K/iYAAA=
MANIFEST_EOF

echo "==> Decoding manifest"
base64 -d "${WORKDIR}/manifest.json.gz.b64" > "${WORKDIR}/manifest.json.gz"
gunzip -f "${WORKDIR}/manifest.json.gz"

echo "==> Adding scripts/migrate_existing_checks_to_partitioned.sh"
if ! python3 "${WORKDIR}/patcher.py" "${WORKDIR}/manifest.json" "${REPO_ROOT}"; then
  echo "==> Skipped one or more files — see messages above." >&2
  exit 1
fi

echo
echo "==> Done. This added a TOOL, not a migration — nothing has run"
echo "    against your database. Next: read"
echo "    scripts/migrate_existing_checks_to_partitioned.sh's header"
echo "    comment, then run it with no flags first (dry-run/plan only)."

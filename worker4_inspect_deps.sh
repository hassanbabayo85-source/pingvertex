#!/usr/bin/env bash
set -euo pipefail

echo "===== ROOT Cargo.toml ====="
cat Cargo.toml

echo
echo "===== APPLICATION Cargo.toml ====="
cat crates/application/Cargo.toml

echo
echo "===== INFRA Cargo.toml ====="
cat crates/infra/Cargo.toml

echo
echo "===== APPLICATION TEST FILES USING INFRA ====="
grep -Rni --include='*.rs' 'use infra\|extern crate infra' crates/application/tests crates/application/src 2>/dev/null || true

echo
echo "===== CARGO DEPENDENCY GRAPH ====="
cargo tree -p application -e normal,dev 2>&1 || true

echo
echo "===== CARGO METADATA PACKAGE RELATIONSHIPS ====="
cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c '
import json,sys
d=json.load(sys.stdin)
for p in d["packages"]:
    if p["name"] in {"application","infra","domain"}:
        print("\\nPACKAGE:", p["name"])
        print("manifest:", p["manifest_path"])
        for dep in p["dependencies"]:
            if dep["name"] in {"application","infra","domain"}:
                print("  ->", dep["name"], "kind=", dep["kind"], "path=", dep.get("path"))
' || true

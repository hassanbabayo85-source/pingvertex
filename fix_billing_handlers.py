#!/usr/bin/env python3
"""
fix_billing_handlers.py

Adds `unused_variables` to the #[allow(...)] attribute above every handler
function whose body begins with `return Err(ApiError::gone(...))`.

This silences the unused-variable warnings on parameters that are only
referenced in the dead code below the early return, WITHOUT renaming any
parameter or touching the dead code itself — so it can't trigger the
"cannot find value" error that makes `cargo fix` roll back its own changes.

Run from the repo root:
    python3 fix_billing_handlers.py
"""

import re
import shutil
from pathlib import Path

FILES = [
    "crates/api/src/handlers/billing_finalization.rs",
    "crates/api/src/handlers/initiate_checkout.rs",
    "crates/api/src/handlers/verify_checkout.rs",
    "crates/api/src/handlers/update_subscription_usage.rs",
    "crates/api/src/handlers/add_on_billing.rs",
    "crates/api/src/handlers/change_plan.rs",
]

FN_SIG_RE = re.compile(r"pub async fn\s+(\w+)\s*\(")
RETIRED_BODY_RE = re.compile(r"return\s+Err\(\s*ApiError::gone\(")
ALLOW_ATTR_RE = re.compile(r"^(\s*)#\[allow\(([^)]*)\)\](\s*\n?)$")


def matching_paren(text: str, open_idx: int):
    depth = 0
    i = open_idx
    n = len(text)
    while i < n:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def matching_brace(text: str, open_idx: int):
    depth = 0
    i = open_idx
    n = len(text)
    while i < n:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def find_attr_block_start(text: str, fn_line_start: int) -> int:
    """Walk upward through consecutive `#[...]` lines directly above the fn."""
    cursor = fn_line_start
    while cursor > 0:
        prev_line_end = cursor - 1  # index of the '\n' separating the two lines
        prev_line_start = text.rfind("\n", 0, prev_line_end) + 1
        prev_line = text[prev_line_start:prev_line_end]
        if re.match(r"^\s*#\[.*\]\s*$", prev_line):
            cursor = prev_line_start
        else:
            break
    return cursor


def process_file(path: Path) -> bool:
    text = path.read_text()
    changed = False
    result = []
    pos = 0
    touched_fns = []

    for m in FN_SIG_RE.finditer(text):
        fn_name = m.group(1)
        fn_name_start = m.start()
        open_paren = m.end() - 1
        close_paren = matching_paren(text, open_paren)
        if close_paren is None:
            continue
        brace_idx = text.find("{", close_paren)
        if brace_idx == -1:
            continue
        body_end = matching_brace(text, brace_idx)
        if body_end is None:
            continue

        body = text[brace_idx + 1 : body_end]
        if not RETIRED_BODY_RE.match(body.lstrip()):
            continue  # not a retired/early-return handler, leave untouched

        fn_line_start = text.rfind("\n", 0, fn_name_start) + 1
        attr_block_start = find_attr_block_start(text, fn_line_start)
        attr_block = text[attr_block_start:fn_line_start]

        new_lines = []
        found_allow = False
        for line in attr_block.splitlines(keepends=True):
            am = ALLOW_ATTR_RE.match(line)
            if am:
                found_allow = True
                pre, inner, tail = am.groups()
                items = [x.strip() for x in inner.split(",") if x.strip()]
                if "unused_variables" not in items:
                    items.append("unused_variables")
                    changed = True
                new_lines.append(f"{pre}#[allow({', '.join(items)})]{tail}")
            else:
                new_lines.append(line)

        if not found_allow:
            indent = text[fn_line_start:fn_name_start]
            new_lines.append(f"{indent}#[allow(unused_variables)]\n")
            changed = True

        touched_fns.append(fn_name)
        result.append(text[pos:attr_block_start])
        result.append("".join(new_lines))
        pos = fn_line_start

    result.append(text[pos:])
    new_text = "".join(result)

    if changed:
        backup = path.with_suffix(path.suffix + ".bak")
        shutil.copy2(path, backup)
        path.write_text(new_text)
        print(f"✔ Updated {path}  (backup: {backup.name})  -> {', '.join(touched_fns)}")
    else:
        print(f"· No changes needed: {path}")

    return changed


def main():
    any_changed = False
    for f in FILES:
        p = Path(f)
        if not p.exists():
            print(f"✘ MISSING: {f}")
            continue
        any_changed |= process_file(p)

    print()
    if any_changed:
        print("Done. Run `cargo check` to confirm the warnings are gone.")
    else:
        print("No files were modified.")


if __name__ == "__main__":
    main()

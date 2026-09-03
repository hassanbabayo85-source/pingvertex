#!/usr/bin/env bash
set -euo pipefail

ADVANCED="apps/leptos-dashboard/src/api/advanced_monitoring.rs"
DEVELOPER="apps/leptos-dashboard/src/api/developer.rs"
NAV="apps/leptos-dashboard/src/components/navigation.rs"
LAYOUT="apps/leptos-dashboard/src/components/layout.rs"

for file in "$ADVANCED" "$DEVELOPER" "$NAV" "$LAYOUT"; do
    if [[ ! -f "$file" ]]; then
        echo "ERROR: Missing expected file: $file"
        exit 1
    fi
done

STAMP="$(date +%Y%m%d-%H%M%S)"

cp "$ADVANCED" "${ADVANCED}.bak_phase1_clippy_${STAMP}"
cp "$DEVELOPER" "${DEVELOPER}.bak_phase1_clippy_${STAMP}"
cp "$NAV" "${NAV}.bak_phase1_clippy_${STAMP}"
cp "$LAYOUT" "${LAYOUT}.bak_phase1_clippy_${STAMP}"

python3 - "$ADVANCED" "$DEVELOPER" "$NAV" "$LAYOUT" <<'PY'
from pathlib import Path
import sys

advanced = Path(sys.argv[1])
developer = Path(sys.argv[2])
nav = Path(sys.argv[3])
layout = Path(sys.argv[4])

# ------------------------------------------------------------
# 1. Remove unused maintenance delete API wrapper.
# ------------------------------------------------------------
text = advanced.read_text()

block = '''pub async fn delete_maintenance(id: &str) -> Result<(), client::ApiError> {
    client::delete_json_no_content(&format!("/maintenance-windows/{id}")).await
}

'''

if block not in text:
    raise SystemExit(
        "ERROR: delete_maintenance block not found; refusing to modify advanced_monitoring.rs"
    )

text = text.replace(block, "", 1)

# ------------------------------------------------------------
# 2. Remove unused escalation delete API wrapper.
# ------------------------------------------------------------
block = '''pub async fn delete_escalation(id: &str) -> Result<(), client::ApiError> {
    client::delete_json_no_content(&format!("/escalation-policies/{id}")).await
}
'''

if block not in text:
    raise SystemExit(
        "ERROR: delete_escalation block not found; refusing to modify advanced_monitoring.rs"
    )

text = text.replace(block, "", 1)
advanced.write_text(text)

# ------------------------------------------------------------
# 3. Remove unused developer flow delete API wrapper.
# ------------------------------------------------------------
text = developer.read_text()

block = '''pub async fn delete_flow(monitor_id: &str, flow_id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/monitors/{monitor_id}/flows/{flow_id}")).await
}
'''

if block not in text:
    raise SystemExit(
        "ERROR: delete_flow block not found; refusing to modify developer.rs"
    )

text = text.replace(block, "", 1)
developer.write_text(text)

# ------------------------------------------------------------
# 4. Remove unused NavStatus::ComingSoon variant.
#
# All current NAV_GROUPS entries are Available. There are no
# current ComingSoon constructors, so retaining the variant
# creates dead-code without providing current behavior.
# ------------------------------------------------------------
text = nav.read_text()

old_variant = '''    /// The feature is planned (owned by another Path) but not routable yet.
    /// Rendered as a disabled, non-navigating list item so it never becomes
    /// a dead link.
    ComingSoon,
'''

if old_variant not in text:
    raise SystemExit(
        "ERROR: Expected NavStatus::ComingSoon variant block not found"
    )

text = text.replace(old_variant, "", 1)

# Update module documentation so it no longer promises a nonexistent
# ComingSoon variant.
text = text.replace(
    '//! integration pass) can flip an item from `NavStatus::ComingSoon` to\n'
    '//! `NavStatus::Available` in one place, in `routes/mod.rs`-adjacent order,\n',
    '//! integration pass) can add a future navigation item in one place, in\n'
    '//! `routes/mod.rs`-adjacent order.\n',
    1,
)

nav.write_text(text)

# ------------------------------------------------------------
# 5. Layout now only needs the Available branch.
# ------------------------------------------------------------
text = layout.read_text()

old_match = '''                                        .map(|item| match item.status {
                                            NavStatus::Available => view! {
                                                <A href=item.href>{item.label}</A>
                                            }.into_any(),
                                            NavStatus::ComingSoon => view! {
                                                <span
                                                    class="sidebar-group-disabled"
                                                    title="Coming soon"
                                                    aria-disabled="true"
                                                >
                                                    {item.label}
                                                    <span class="sidebar-soon-badge">"soon"</span>
                                                </span>
                                            }.into_any(),
                                        })
'''

new_match = '''                                        .map(|item| match item.status {
                                            NavStatus::Available => view! {
                                                <A href=item.href>{item.label}</A>
                                            }.into_any(),
                                        })
'''

if old_match not in text:
    raise SystemExit(
        "ERROR: Expected NavStatus match block not found in layout.rs"
    )

text = text.replace(old_match, new_match, 1)
layout.write_text(text)
PY

cargo fmt --all

echo
echo "=============================================="
echo "VERIFYING REMAINING CLIPPY FIXES"
echo "=============================================="

echo
echo "--- delete_maintenance / delete_escalation ---"
grep -n 'delete_maintenance\|delete_escalation' "$ADVANCED" || true

echo
echo "--- delete_flow ---"
grep -n 'delete_flow' "$DEVELOPER" || true

echo
echo "--- ComingSoon ---"
grep -RIn 'ComingSoon' \
    apps/leptos-dashboard/src/components/navigation.rs \
    apps/leptos-dashboard/src/components/layout.rs || true

echo
echo "=============================================="
echo "RUNNING CLIPPY"
echo "=============================================="

cargo clippy --workspace --all-targets -- -D warnings

echo
echo "=============================================="
echo "CLIPPY PASSED"
echo "=============================================="

echo
echo "Next verification:"
echo "  cargo test --workspace"
echo "  cd apps/leptos-dashboard && trunk build --release"

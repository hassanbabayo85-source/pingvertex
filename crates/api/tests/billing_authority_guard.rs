//! Single billing authority guard test.
//!
//! This test enforces the single-billing-authority SECURITY INVARIANT:
//!
//!   There is exactly ONE runtime source of truth for
//!   "What is this tenant allowed to do?" and "How much does it cost?"
//!
//! After TASK 2 that source is `EntitlementResolver::resolve(tenant)`,
//! which reads `plan_entitlements` for the tenant's current plan. The
//! legacy `subscription_repository` is now only an *input* to the
//! resolver, not a decision-maker itself, and may only appear in:
//!
//!   * the port definition and its re-export,
//!   * `AppState` wiring,
//!   * retirement shells that return HTTP 410,
//!   * the two webhook handlers that must be able to route historical
//!     subscription webhook payloads (`billing_webhook.rs`,
//!     `paystack_webhook.rs`) — but only as far as the use case, which
//!     itself only acts on wallet top-up events,
//!   * tests and fixtures.
//!
//! A new runtime handler that reaches for `subscription_repository` to
//! make a billing or entitlement decision fails this test. That is the
//! point: it forces the author to ask "should this go through
//! `EntitlementResolver` instead?".
//!
//! The test matches the *field name* `subscription_repository` (the
//! `AppState` field), not the trait name `SubscriptionRepository`, so
//! the port / infra impl / application use-case files that carry the
//! trait bound are unaffected. `add_on_subscription_repository` is
//! deliberately NOT a match (preceding byte is `_`).

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str())
                && (name == "target" || name.starts_with('.'))
            {
                continue;
            }
            walk_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn mentions_subscription_repository(content: &str) -> bool {
    let needle = "subscription_repository";
    let bytes = content.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = content[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let prev_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let next_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if prev_ok && next_ok {
            return true;
        }
        from = end;
    }
    false
}

const ALLOWED_PATHS: &[&str] = &[
    // AppState field + builder wiring.
    "crates/api/src/state.rs",
    "crates/api/src/main.rs",
    // Retirement shell. Must return `ApiError::gone` (HTTP 410);
    // enforced by `retirement_shells_return_410` below.
    "crates/api/src/handlers/update_subscription_usage.rs",
    // Webhook handlers. These forward historical subscription payloads
    // into `ProcessBillingWebhookUseCase`; the use case itself only
    // acts on `wallet_top_up=true` events and falls through to the
    // retired subscription branch otherwise.
    "crates/api/src/handlers/billing_webhook.rs",
    "crates/api/src/handlers/paystack_webhook.rs",
    // Task 6 — subscription checkout / lifecycle. This handler uses
    // `subscription_repository` as *input* to `CancelSubscriptionUseCase`
    // and `VerifySubscriptionCheckoutUseCase`; it does NOT read the
    // subscription to authorize requests. The authority for numeric
    // limits remains `EntitlementResolver`.
    "crates/api/src/handlers/subscription_billing.rs",
];

#[test]
fn no_runtime_handler_introduces_a_new_subscription_repository_use() {
    let root = workspace_root();
    let scan_roots = [root.join("crates/api/src"), root.join("crates/worker/src")];

    let mut files = Vec::new();
    for scan_root in &scan_roots {
        walk_rs_files(scan_root, &mut files);
    }

    let mut violations: Vec<String> = Vec::new();
    for path in &files {
        let Ok(rel) = path.strip_prefix(&root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if !mentions_subscription_repository(&content) {
            continue;
        }

        if !ALLOWED_PATHS.iter().any(|a| rel_str == *a) {
            violations.push(rel_str);
        }
    }

    violations.sort();
    violations.dedup();

    assert!(
        violations.is_empty(),
        "BILLING AUTHORITY VIOLATION: `subscription_repository` is referenced in \
         files under crates/api/src or crates/worker/src that are not in \
         the explicit allowlist in this test.\n\n\
         As of TASK 2 the authority is `EntitlementResolver`. New runtime \
         entitlement code MUST use:\n  \
           * EntitlementResolver::resolve(tenant)  (quotas and minimums)\n  \
           * domain::Entitlements                  (the returned snapshot)\n\n\
         If the new file is a genuine retirement shell, add it to \
         ALLOWED_PATHS in this test AND ensure the handler returns \
         `ApiError::gone` unconditionally.\n\n\
         Offending files:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn retirement_shells_return_410() {
    let root = workspace_root();
    let shells = ["crates/api/src/handlers/update_subscription_usage.rs"];

    for shell in shells {
        let path = root.join(shell);
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {shell}: {e}"));
        assert!(
            content.contains("ApiError::gone"),
            "BILLING AUTHORITY VIOLATION: `{shell}` is a retirement shell but does \
             not return `ApiError::gone` (HTTP 410)."
        );
    }
}

#[test]
fn billing_authority_module_is_declared() {
    let root = workspace_root();
    let candidate = root.join("crates/application/src/billing_authority.rs");
    assert!(
        candidate.exists(),
        "BILLING AUTHORITY VIOLATION: crates/application/src/billing_authority.rs is \
         missing — the canonical authority doc module must exist."
    );
}

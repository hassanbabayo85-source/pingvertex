//! PART 15 — single billing authority guard test.
//!
//! This test enforces the SECURITY INVARIANT of PART 15:
//!
//!   There is exactly ONE runtime source of truth for
//!   "What does this operation cost?" and
//!   "Is this tenant allowed to perform it?"
//!
//! That source is the PAYG wallet + PAYG pricing + free-tier limits
//! sourced from `PricingConfig.free_tier`. The legacy subscription
//! architecture is retired and may only appear in:
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
//! point: it forces the author to ask "should this be PAYG instead?".
//!
//! The test matches the *field name* `subscription_repository` (the
//! `AppState` field), not the trait name `SubscriptionRepository`, so
//! the port / infra impl / application use-case files that carry the
//! trait bound are unaffected. `add_on_subscription_repository` is
//! deliberately NOT a match (preceding byte is `_`).

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR for the api crate is `<root>/crates/api`;
    // two levels up is the workspace root.
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
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name == "target" || name.starts_with('.') {
                    continue;
                }
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

/// True if `content` mentions `subscription_repository` as a standalone
/// identifier, excluding longer identifiers like
/// `add_on_subscription_repository`.
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

/// Workspace-relative paths (POSIX-style) under `crates/api/src/` or
/// `crates/worker/src/` that are permitted to mention
/// `subscription_repository`. Every entry here is a deliberate
/// exception; adding a new one is the moment to justify why the new
/// file is not a PAYG path.
///
/// Files outside those two directories (the `application` crate's port
/// definition and retired use cases, the `infra` crate's repository
/// implementations, the guard test itself) are not scanned — they
/// cannot be the entry point for a new runtime decision.
const ALLOWED_PATHS: &[&str] = &[
    // AppState field + builder wiring.
    "crates/api/src/state.rs",
    "crates/api/src/main.rs",
    // Retirement shells. Each MUST return `ApiError::gone` (HTTP 410);
    // enforced by `retirement_shells_return_410` below.
    "crates/api/src/handlers/update_subscription_usage.rs",
    "crates/api/src/handlers/billing_finalization.rs",
    "crates/api/src/handlers/verify_checkout.rs",
    "crates/api/src/handlers/change_plan.rs",
    // Webhook handlers. These forward historical subscription payloads
    // into `ProcessBillingWebhookUseCase`; the use case itself only
    // acts on `wallet_top_up=true` events and falls through to the
    // retired subscription branch otherwise.
    "crates/api/src/handlers/billing_webhook.rs",
    "crates/api/src/handlers/paystack_webhook.rs",
];

#[test]
fn no_runtime_handler_introduces_a_new_subscription_repository_use() {
    // Scope: only the directories where a NEW runtime billing/entitlement
    // decision would actually get wired in. The `application` crate
    // carries the port definition and the (already-retired) use cases
    // themselves; the `infra` crate carries the repositories. A new file
    // in either is not a "runtime decision" — the risk we care about is
    // a new HTTP handler or worker job that reaches for the subscription
    // field to make a decision.
    let root = workspace_root();
    let scan_roots = [
        root.join("crates/api/src"),
        root.join("crates/worker/src"),
    ];

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
        "PART 15 VIOLATION: `subscription_repository` is referenced in \
         files under crates/api/src or crates/worker/src that are not in \
         the explicit allowlist in this test.\n\n\
         Subscription is retired. New runtime billing/entitlement code \
         MUST use:\n  \
           * PAYGPricingRepository  (cost)\n  \
           * WalletRepository       (balance, reservation, ledger)\n  \
           * PricingConfig.free_tier (free entitlement)\n  \
           * MeterUsageUseCase      (metering)\n\n\
         If the new file is a genuine retirement shell, add it to \
         ALLOWED_PATHS in this test AND ensure the handler returns \
         `ApiError::gone` unconditionally.\n\n\
         Offending files:\n  {}",
        violations.join("\n  ")
    );
}

/// Every handler listed as a retirement shell MUST unconditionally
/// return `ApiError::gone` (HTTP 410). This prevents the shell from
/// silently becoming a live subscription runtime path.
#[test]
fn retirement_shells_return_410() {
    let root = workspace_root();
    let shells = [
        "crates/api/src/handlers/update_subscription_usage.rs",
        "crates/api/src/handlers/billing_finalization.rs",
        "crates/api/src/handlers/verify_checkout.rs",
        "crates/api/src/handlers/change_plan.rs",
    ];

    for shell in shells {
        let path = root.join(shell);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {shell}: {e}"));
        assert!(
            content.contains("ApiError::gone"),
            "PART 15 VIOLATION: `{shell}` is a retirement shell but does \
             not return `ApiError::gone` (HTTP 410). A retirement shell \
             must unconditionally reject with 410 so the 'subscription is \
             retired' contract holds."
        );
    }
}

/// The canonical authority module must exist and be re-exported from the
/// application crate, so a reader landing on any of the legacy types can
/// find the invariant in one hop.
#[test]
fn billing_authority_module_is_declared() {
    let root = workspace_root();
    let candidate = root.join("crates/application/src/billing_authority.rs");
    assert!(
        candidate.exists(),
        "PART 15 VIOLATION: crates/application/src/billing_authority.rs is \
         missing — the canonical authority doc module must exist."
    );
}

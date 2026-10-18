//! Acceptance test: `monitoring/rules.yml` defines every alert the
//! observability contract requires.
//!
//! This test does NOT parse YAML (which would need a new dependency). It
//! reads the file as text and asserts that each required alert name is
//! present. That is enough to catch the realistic failure mode: someone
//! deletes or renames an alert rule and the corresponding production
//! signal silently stops firing.
//!
//! The contract checked here is:
//!
//!   1. Availability of the two services (`ApiDown`, `WorkerDown`)
//!   2. Worker check failures
//!   3. Financial reconciliation (`BillingReconciliationMismatch`,
//!      `BillingReconciliationAnomalies`, `PaygReconciliationAnomalies`)
//!   4. Auto top-up reconciliation + repeated provider failures
//!   5. PAYG billing failures + pricing resolution failures
//!   6. Queue growth (bulk imports and scheduler due-queue)
//!   7. Scheduler retry storm, backpressure, tenant starvation

use std::fs;
use std::path::PathBuf;

fn rules_path() -> PathBuf {
    // CARGO_MANIFEST_DIR for this test is crates/api.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root is two levels above crates/api")
        .join("monitoring/rules.yml")
}

#[test]
fn rules_yml_defines_every_required_production_alert() {
    let path = rules_path();
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    // Every alert the observability contract requires. This list is
    // the single source of truth for "which alerts must exist"; if a new
    // alert is required, add it here as well as in rules.yml.
    let required = [
        // Availability
        "ApiDown",
        "WorkerDown",
        "WorkerFailures",
        // Financial reconciliation
        "BillingReconciliationMismatch",
        "BillingReconciliationAnomalies",
        "PaygReconciliationAnomalies",
        // Auto top-up
        "AutoTopUpReconciliationRequired",
        "AutoTopUpPaymentFailures",
        // PAYG billing
        "PaygBillingFailures",
        "PaygPricingResolutionFailure",
        "PaygPricingMissingPrice",
        // Queues
        "BulkQueueGrowth",
        "SchedulerDueQueueGrowth",
        // Scheduler
        "SchedulerRetryStorm",
        "SchedulerBackpressure",
        "TenantStarvation",
    ];

    let mut missing = Vec::new();
    for name in &required {
        // Match `- alert: Name` exactly, so that a substring match on a
        // different alert's name does not accidentally satisfy this check.
        let needle = format!("alert: {name}");
        if !body.contains(&needle) {
            missing.push(*name);
        }
    }

    assert!(
        missing.is_empty(),
        "monitoring/rules.yml is missing the following required alerts: {missing:?}"
    );
}

/// Every alert must carry an explicit `expr:` line, and none may be empty.
/// This catches the common editing mistake of writing an alert without a
/// rule body.
#[test]
fn every_alert_has_an_expression() {
    let path = rules_path();
    let body = fs::read_to_string(&path).expect("read rules.yml");

    let mut alerts_seen = 0usize;
    let mut current_alert: Option<String> = None;
    let mut current_has_expr = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(name) = trimmed.strip_prefix("- alert:") {
            // Close out the previous alert.
            if let Some(prev) = current_alert.take() {
                assert!(current_has_expr, "alert `{prev}` has no `expr:` line");
            }
            current_alert = Some(name.trim().to_owned());
            current_has_expr = false;
            alerts_seen += 1;
        } else if trimmed.starts_with("expr:") {
            let value = trimmed.trim_start_matches("expr:").trim();
            assert!(
                !value.is_empty(),
                "alert `{}` has an empty `expr:` value",
                current_alert.as_deref().unwrap_or("<unknown>")
            );
            current_has_expr = true;
        }
    }
    if let Some(prev) = current_alert.take() {
        assert!(current_has_expr, "alert `{prev}` has no `expr:` line");
    }

    assert!(
        alerts_seen >= 16,
        "expected at least 16 alerts in rules.yml, found {alerts_seen}"
    );
}

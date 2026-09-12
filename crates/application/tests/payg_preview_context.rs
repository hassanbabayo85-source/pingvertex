//! PAYG preview contextual pricing tests.
//!
//! Task 1 acceptance: the preview path and the actual billing path must
//! resolve the **same** contextual price row for the **same** tenant
//! context.
//!
//! These tests live in `tests/` (not in the inline `#[cfg(test)]` module
//! inside `calculate_payg_preview.rs`) because they construct an
//! `InMemoryPAYGPricingRepository` from the `infra` crate. `application`
//! has `infra` only as a `[dev-dependency]`, and Rust does not let a
//! crate's own *library* tests reach into a dev-dependency when the
//! dev-dependency itself depends on the crate (circular dev-dependency).
//! External integration tests under `tests/` do not hit this limitation.
//!
//! Before Task 1, the preview path left `PricingContext::account_type`
//! at `None`, which meant a company tenant was quoted the *generic*
//! price while the actual billing path (which already used
//! `tenant.account_type`) would debit at the *company* price. This test
//! file locks in the fix.

use application::ports::{
    PAYGPricingRepository, PricingContext, PricingRuleContext, resolve_price,
};
use application::use_cases::calculate_payg_preview::{
    CalculatePaygPreviewUseCase, PaygPreviewInput,
};
use chrono::{Duration as ChronoDuration, Utc};
use domain::{AccountType, MonitorKind};
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
use std::sync::Arc;

async fn seed_company_and_generic_rules(repo: &Arc<InMemoryPAYGPricingRepository>) {
    let effective_from = Utc::now() - ChronoDuration::minutes(1);

    // Company-only rule with a distinctive price.
    let company_rule = PricingRuleContext {
        account_type: Some(AccountType::Company),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.monitoring",
        "check",
        100, // company = 100 units per check
        "USD",
        effective_from,
        &company_rule,
    )
    .await
    .unwrap();

    // Generic rule with a very different price, so an accidental
    // fallback to the generic row is immediately detectable.
    repo.create_version_with_context(
        "http_check.monitoring",
        "check",
        2, // generic = 2 units per check
        "USD",
        effective_from,
        &PricingRuleContext::default(),
    )
    .await
    .unwrap();
}

/// Task 1 acceptance: for the same trusted context, the preview path and
/// the actual billing path must resolve the same `(unit_price, version)`.
#[tokio::test]
async fn preview_matches_actual_billing_for_company_context() {
    let repo = Arc::new(InMemoryPAYGPricingRepository::new());
    seed_company_and_generic_rules(&repo).await;

    // Preview as a company tenant.
    let preview = CalculatePaygPreviewUseCase::new(repo.clone())
        .execute(PaygPreviewInput {
            account_type: Some(AccountType::Company),
            monitors: 1,
            interval_seconds: 300,
            regions: 1,
            forecast_days: 1,
            kind: MonitorKind::Http,
        })
        .await
        .unwrap();

    assert_eq!(
        preview.unit_price, 100,
        "preview must use the company-contextual price, not the generic one"
    );

    // Now resolve the price the actual billing path would use for the
    // same trusted context (this is exactly what `one_time_check.rs` and
    // `durable_scheduler.rs` construct at billing time).
    let billing_price = resolve_price(
        repo.as_ref(),
        "http_check.monitoring",
        &PricingContext {
            account_type: Some(AccountType::Company),
            region: None,
            monitor_kind: Some(MonitorKind::Http),
            quantity: 1,
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        billing_price.unit_price, preview.unit_price,
        "preview and actual billing must resolve the same unit price for the same context"
    );
    assert_eq!(
        billing_price.version, preview.pricing_version,
        "preview and actual billing must pin the same pricing version"
    );
}

/// Task 1 regression: a company tenant's preview must differ from a
/// generic preview when contextual pricing rules exist. This is the
/// concrete symptom the fix addresses.
#[tokio::test]
async fn company_preview_differs_from_generic_preview() {
    let repo = Arc::new(InMemoryPAYGPricingRepository::new());
    seed_company_and_generic_rules(&repo).await;

    let company_preview = CalculatePaygPreviewUseCase::new(repo.clone())
        .execute(PaygPreviewInput {
            account_type: Some(AccountType::Company),
            monitors: 1,
            interval_seconds: 300,
            regions: 1,
            forecast_days: 1,
            kind: MonitorKind::Http,
        })
        .await
        .unwrap();

    let generic_preview = CalculatePaygPreviewUseCase::new(repo)
        .execute(PaygPreviewInput {
            account_type: None,
            monitors: 1,
            interval_seconds: 300,
            regions: 1,
            forecast_days: 1,
            kind: MonitorKind::Http,
        })
        .await
        .unwrap();

    assert_ne!(
        company_preview.unit_price, generic_preview.unit_price,
        "company and generic previews must differ when contextual rules exist"
    );
    assert_eq!(company_preview.unit_price, 100);
    assert_eq!(generic_preview.unit_price, 2);
}

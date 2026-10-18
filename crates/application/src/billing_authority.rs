//! # The ONE canonical entitlement and billing authority
//!
//! SECURITY INVARIANT:
//!
//! > There must be exactly ONE source of truth for
//! > "What is this tenant allowed to do?" and
//! > "How much does it cost?"
//!
//! ## The authorities (after TASK 2)
//!
//! | Question                  | Authority                                                       |
//! |---------------------------|-----------------------------------------------------------------|
//! | Is this tenant allowed?   | `EntitlementResolver::resolve(tenant)` → `domain::Entitlements` |
//! | What does a plan cost?    | `plan_prices` table (versioned, read via `PlanPrice`)           |
//! | Historical PAYG ledger    | `WalletRepository` (read-only compatibility)                    |
//!
//! Every runtime entitlement decision MUST go through
//! `EntitlementResolver`. The resolver reads the latest
//! `plan_entitlements` rows for the tenant's current plan (or `Free`
//! if the tenant has no active subscription). It is not cached: quota
//! checks that race against concurrent writes are serialized by the
//! repository's advisory lock
//! (`PostgresMonitorRepository::save_if_within_quota`).
//!
//! ## What is NOT the authority (but is retained as input)
//!
//! The following types are kept for migration and historical reads.
//! They MUST NEVER participate in a *new* runtime entitlement decision:
//!
//! * [`domain::Subscription`] — an *input* to the resolver, not the decision itself.
//! * [`domain::PlanTier`] — ditto; the resolver maps tier → plan id → entitlements.
//! * [`crate::ports::SubscriptionRepository`] — data access only.
//! * [`crate::use_cases::ResolveEntitlementsUseCase`] — retired; unused.
//!
//! ## What is retired (TASK 8 will remove the remaining code paths)
//!
//! The following still exist in the source tree for historical
//! compatibility but MUST NOT influence any new entitlement decision:
//!
//! * PAYG wallet balance as an access check.
//! * `PricingConfig.free_tier` as a monitor limit.
//! * Automatic per-usage charges and automatic top-ups.
//!
//! TASK 8 removes the runtime paths. Until then, only these
//! **compatibility** paths may read them (and only for historical or
//! read-only purposes):
//!
//! * `billing_webhook.rs` / `paystack_webhook.rs` — must route historical
//!   subscription webhook payloads.
//! * `update_subscription_usage.rs` — retirement shell that returns HTTP 410.
//!
//! ## How this is enforced
//!
//! `crates/api/tests/billing_authority_guard.rs` scans `crates/api/src`
//! and `crates/worker/src` for references to `subscription_repository`.
//! It fails the build if a file outside an explicit allowlist mentions
//! it. Adding a new runtime path that reaches for subscription state to
//! make a decision requires editing that allowlist — the exact moment
//! to ask "should this go through `EntitlementResolver` instead?".
//!
//! [`domain::Subscription`]: domain::Subscription
//! [`domain::PlanTier`]: domain::enums::PlanTier
//! [`crate::ports::SubscriptionRepository`]: crate::ports::SubscriptionRepository
//! [`crate::use_cases::ResolveEntitlementsUseCase`]: crate::use_cases::ResolveEntitlementsUseCase

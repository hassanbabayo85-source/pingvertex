//! # The ONE canonical billing authority
//!
//! SECURITY INVARIANT (PART 15):
//!
//! > There must be exactly ONE source of truth for
//! > "What does this operation cost?" and
//! > "Is this tenant allowed to perform it?"
//!
//! This module names that authority and forbids the alternative.
//!
//! ## The authority
//!
//! | Question                    | Authority                                                      |
//! |-----------------------------|----------------------------------------------------------------|
//! | What does X cost?           | `PAYGPricingRepository` (via `resolve_price`, keyed by `PaygOperation`) |
//! | Is this tenant allowed?     | `WalletRepository::get_balance_summary` + `PricingConfig.free_tier` |
//! | How is a decision recorded? | `WalletRepository` (Wallet / CreditReservation / WalletTransaction) |
//! | How is usage metered?       | `UsageEvent` + `MeterUsageUseCase`                              |
//! | Who arbitrates races?       | `reserve` / `capture` / `release` under `SELECT … FOR UPDATE`   |
//!
//! Concretely: every billable runtime operation must go through
//! `MeterUsageUseCase` (or `ChargeForUsageUseCase`) against a
//! `WalletRepository`. There is no second path.
//!
//! ## What is NOT the authority
//!
//! The following types exist ONLY for migration, historical reads, and
//! compatibility shells (which return HTTP 410). They must **never**
//! participate in a runtime decision about cost or entitlement:
//!
//! * [`domain::Subscription`]
//! * [`domain::PlanTier`]
//! * [`domain::Plan`]
//! * [`crate::ports::SubscriptionRepository`]
//! * [`crate::ports::SubscriptionUsagePeriodRepository`]
//! * [`crate::ports::SubscriptionUsageTransaction`]
//! * [`crate::use_cases::ResolveEntitlementsUseCase`]
//!
//! ## How this is enforced
//!
//! The test `crates/api/tests/part15_billing_authority.rs` scans the
//! workspace for any file (outside an explicit allowlist) that mentions
//! the `subscription_repository` field, and fails the build if a new
//! occurrence appears. Adding a new runtime path that reads subscription
//! state requires editing that allowlist — which is exactly the moment
//! to ask "should this be PAYG instead?".
//!
//! ## Free-tier limits
//!
//! The "free entitlement" (a bounded number of monitors at a bounded
//! minimum interval, with bounded retention, that a tenant may use
//! without funding the wallet) is NOT a subscription concept. It is
//! sourced from `PricingConfig.free_tier`, the same versioned
//! append-only configuration that carries PAYG rates. The historical
//! `Plan::for_tier(PlanTier::Free)` values are kept only for
//! migration-compatibility checks in tests; runtime code that still
//! calls it (see the guard test's allowlist) is expected to migrate to
//! `PricingConfig.free_tier` and is not a second authority: both read
//! the same source of truth once the migration lands.
//!
//! [`domain::Subscription`]: domain::Subscription
//! [`domain::PlanTier`]: domain::enums::PlanTier
//! [`domain::Plan`]: domain::Plan
//! [`crate::ports::SubscriptionRepository`]: crate::ports::SubscriptionRepository
//! [`crate::ports::SubscriptionUsagePeriodRepository`]: crate::ports::SubscriptionUsagePeriodRepository
//! [`crate::ports::SubscriptionUsageTransaction`]: crate::ports::SubscriptionUsageTransaction
//! [`crate::use_cases::ResolveEntitlementsUseCase`]: crate::use_cases::ResolveEntitlementsUseCase

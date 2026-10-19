//! Task 8B — plan change for fixed-price subscriptions.
//!
//! Differences from the PAYG-era implementation:
//!   * Prices come from `plan_prices` (monthly rows), not `PricingConfig`.
//!   * Downgrades are rejected. In a subscription model, a downgrade
//!     should take effect at the end of the current period, not mid-cycle
//!     with a wallet credit.
//!   * No wallet is touched. There is no "proration credit" any more.
//!
//! This use case is not yet wired to an HTTP handler. Task 7 will do so
//! on top of the Task 6 subscription-billing surface.

use crate::ports::*;
use chrono::{DateTime, Utc};
use domain::money::{BillingInterval, Currency, Money};
use domain::proration::{calculate_proration_from_money, money_to_decimal};
use domain::{
    CheckoutSession, Plan, PlanTier, SubscriptionPlanChange, SubscriptionPlanChangeStatus,
    SubscriptionStatus, SubscriptionUsagePeriod,
};
use rust_decimal::Decimal;
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;

#[derive(Debug)]
pub enum ChangePlanError {
    SubscriptionNotFound,
    SamePlan,
    /// Downgrades must be scheduled at period end, not executed mid-cycle
    /// with a wallet credit. Reject at the API boundary with 409.
    DowngradesRequirePeriodEnd,
    Invalid(String),
    PriceNotFound,
    Pricing(domain::proration::ProrationError),
    Provider(Box<dyn Error + Send + Sync>),
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for ChangePlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriptionNotFound => write!(f, "subscription not found"),
            Self::SamePlan => write!(f, "requested plan is already active"),
            Self::DowngradesRequirePeriodEnd => write!(
                f,
                "downgrades take effect at the end of the current billing period; \
                 contact support to schedule one"
            ),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::PriceNotFound => {
                write!(f, "no effective price configured for the requested plan/currency")
            }
            Self::Pricing(e) => write!(f, "proration error: {e}"),
            Self::Provider(e) => write!(f, "{e}"),
            Self::Repository(e) => write!(f, "{e}"),
        }
    }
}

impl Error for ChangePlanError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePlanOutcome {
    pub plan_change_id: Uuid,
    pub calculated_amount: Decimal,
    pub checkout_session_id: Option<Uuid>,
    pub checkout_url: Option<String>,
    pub applied: bool,
}

pub struct ChangeSubscriptionPlanUseCase<T> {
    subs: Arc<dyn SubscriptionRepository>,
    prices: Arc<dyn PlanPriceRepository>,
    changes: Arc<dyn SubscriptionPlanChangeRepository>,
    sessions: Arc<dyn CheckoutSessionRepository>,
    provider: Arc<dyn PaymentProvider>,
    periods: Arc<dyn SubscriptionUsagePeriodRepository>,
    tx: T,
    ids: Arc<dyn IdGenerator>,
}

impl<T: SubscriptionUsageTransaction> ChangeSubscriptionPlanUseCase<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subs: Arc<dyn SubscriptionRepository>,
        prices: Arc<dyn PlanPriceRepository>,
        changes: Arc<dyn SubscriptionPlanChangeRepository>,
        sessions: Arc<dyn CheckoutSessionRepository>,
        provider: Arc<dyn PaymentProvider>,
        periods: Arc<dyn SubscriptionUsagePeriodRepository>,
        tx: T,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            subs,
            prices,
            changes,
            sessions,
            provider,
            periods,
            tx,
            ids,
        }
    }

    pub async fn execute(
        &self,
        tenant: domain::shared::ids::TenantId,
        new_plan: PlanTier,
        currency: Currency,
    ) -> Result<ChangePlanOutcome, ChangePlanError> {
        if matches!(new_plan, PlanTier::Free | PlanTier::Enterprise) {
            return Err(ChangePlanError::Invalid(
                "Free and Enterprise are not subject to self-service plan changes".into(),
            ));
        }

        let sub = self
            .subs
            .find_by_tenant_id(tenant)
            .await
            .map_err(ChangePlanError::Repository)?
            .ok_or(ChangePlanError::SubscriptionNotFound)?;

        if sub.plan_tier == new_plan {
            return Err(ChangePlanError::SamePlan);
        }
        if sub.status != SubscriptionStatus::Active {
            return Err(ChangePlanError::Invalid(
                "only an active subscription can change plan mid-cycle".into(),
            ));
        }

        // Load both prices at MONTHLY interval; the plan change prorates the
        // monthly price regardless of the tenant's current annual/monthly
        // cadence.
        let old_price = self
            .prices
            .find_effective(sub.plan_tier, currency, BillingInterval::Monthly)
            .await
            .map_err(ChangePlanError::Repository)?
            .ok_or(ChangePlanError::PriceNotFound)?;
        let new_price = self
            .prices
            .find_effective(new_plan, currency, BillingInterval::Monthly)
            .await
            .map_err(ChangePlanError::Repository)?
            .ok_or(ChangePlanError::PriceNotFound)?;

        let end = sub
            .current_period_ends_at
            .ok_or_else(|| {
                ChangePlanError::Invalid(
                    "subscription has no current period end; cannot prorate".into(),
                )
            })?;
        let start = sub.created_at;

        let r = calculate_proration_from_money(
            sub.plan_tier,
            new_plan,
            old_price.amount,
            new_price.amount,
            start,
            end,
            Utc::now(),
            new_price.version,
        )
        .map_err(ChangePlanError::Pricing)?;

        if r.calculated_amount.amount_minor < 0 {
            return Err(ChangePlanError::DowngradesRequirePeriodEnd);
        }

        // SubscriptionPlanChange.pricing_version is u32; plan_prices.version is i64.
        let pricing_version = u32::try_from(r.price_version).map_err(|_| {
            ChangePlanError::Invalid("plan price version out of u32 range".into())
        })?;

        let mut c = SubscriptionPlanChange {
            id: Uuid::new_v4(),
            subscription_id: sub.id.0,
            tenant_id: tenant.0,
            old_plan: r.old_plan,
            new_plan: r.new_plan,
            period_start: r.period_start,
            period_end: r.period_end,
            calculated_amount: money_to_decimal(r.calculated_amount),
            currency: currency.as_str().to_owned(),
            pricing_version,
            status: SubscriptionPlanChangeStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
        };
        self.changes
            .create(&c)
            .await
            .map_err(ChangePlanError::Repository)?;

        if r.calculated_amount.amount_minor > 0 {
            let co = self
                .provider
                .create_checkout(&CheckoutRequest {
                    tenant_id: tenant,
                    plan_tier: Some(new_plan),
                    amount_minor: r.calculated_amount.amount_minor,
                    currency: currency.as_str().to_owned(),
                    customer_email: None,
                })
                .await
                .map_err(ChangePlanError::Provider)?;
            let mut s = CheckoutSession::new_pending(
                domain::CheckoutSessionId::new(),
                tenant,
                Some(new_plan),
                self.provider.provider_name().into(),
            );
            s.attach_provider_checkout_id(co.provider_checkout_id);
            s.attach_plan_change(
                c.id,
                money_to_decimal(r.calculated_amount),
                currency.as_str().to_owned(),
            );
            self.sessions
                .create(&s)
                .await
                .map_err(ChangePlanError::Repository)?;
            return Ok(ChangePlanOutcome {
                plan_change_id: c.id,
                calculated_amount: money_to_decimal(r.calculated_amount),
                checkout_session_id: Some(s.id.0),
                checkout_url: Some(co.checkout_url),
                applied: false,
            });
        }

        // calculated_amount == 0 — apply immediately (e.g. flat-rate tiers
        // priced identically for the remaining fraction; extremely rare,
        // but the code path is kept for correctness).
        self.apply(&sub, new_plan, end, pricing_version).await?;
        c.status = SubscriptionPlanChangeStatus::Completed;
        c.completed_at = Some(Utc::now());
        self.changes
            .update(&c)
            .await
            .map_err(ChangePlanError::Repository)?;
        Ok(ChangePlanOutcome {
            plan_change_id: c.id,
            calculated_amount: Decimal::ZERO,
            checkout_session_id: None,
            checkout_url: None,
            applied: true,
        })
    }

    pub async fn apply_verified_change(
        &self,
        tenant: domain::shared::ids::TenantId,
        session_id: Uuid,
    ) -> Result<ChangePlanOutcome, ChangePlanError> {
        let s = self
            .sessions
            .find_by_id(domain::CheckoutSessionId(session_id))
            .await
            .map_err(ChangePlanError::Repository)?
            .ok_or_else(|| ChangePlanError::Invalid("checkout session not found".into()))?;
        if s.tenant_id != tenant || s.plan_change_id.is_none() {
            return Err(ChangePlanError::Invalid(
                "checkout session not found".into(),
            ));
        }
        let cid = s.plan_change_id.unwrap();
        if s.status == domain::CheckoutStatus::Completed {
            return Ok(ChangePlanOutcome {
                plan_change_id: cid,
                calculated_amount: s.amount.unwrap_or(Decimal::ZERO),
                checkout_session_id: Some(session_id),
                checkout_url: None,
                applied: true,
            });
        }
        match self
            .provider
            .verify_payment(
                s.provider_checkout_id
                    .as_deref()
                    .ok_or_else(|| ChangePlanError::Invalid("checkout is not ready".into()))?,
            )
            .await
            .map_err(ChangePlanError::Provider)?
        {
            PaymentVerification::Pending { .. } => Ok(ChangePlanOutcome {
                plan_change_id: cid,
                calculated_amount: s.amount.unwrap_or(Decimal::ZERO),
                checkout_session_id: Some(session_id),
                checkout_url: None,
                applied: false,
            }),
            PaymentVerification::Failed { .. } => {
                let mut c = self
                    .changes
                    .find_by_id(cid, tenant.0)
                    .await
                    .map_err(ChangePlanError::Repository)?
                    .ok_or_else(|| ChangePlanError::Invalid("plan change not found".into()))?;
                c.status = SubscriptionPlanChangeStatus::Failed;
                self.changes
                    .update(&c)
                    .await
                    .map_err(ChangePlanError::Repository)?;
                let mut f = s.clone();
                f.mark_failed();
                self.sessions
                    .update(&f)
                    .await
                    .map_err(ChangePlanError::Repository)?;
                Err(ChangePlanError::Invalid("payment failed".into()))
            }
            PaymentVerification::Paid { .. } => {
                let c = self
                    .changes
                    .find_by_id(cid, tenant.0)
                    .await
                    .map_err(ChangePlanError::Repository)?
                    .ok_or_else(|| ChangePlanError::Invalid("plan change not found".into()))?;

                if c.status != SubscriptionPlanChangeStatus::Pending {
                    return Err(ChangePlanError::Invalid(
                        "plan change is no longer pending".into(),
                    ));
                }

                let sub = self
                    .subs
                    .find_by_tenant_id(tenant)
                    .await
                    .map_err(ChangePlanError::Repository)?
                    .ok_or(ChangePlanError::SubscriptionNotFound)?;

                if c.subscription_id != sub.id.0 {
                    return Err(ChangePlanError::Invalid(
                        "plan change does not belong to the current subscription".into(),
                    ));
                }
                if c.old_plan != sub.plan_tier {
                    return Err(ChangePlanError::Invalid(
                        "subscription plan changed since this plan change was created".into(),
                    ));
                }

                self.apply(&sub, c.new_plan, c.period_end, c.pricing_version)
                    .await?;
                let mut d = s.clone();
                d.mark_completed();
                self.sessions
                    .update(&d)
                    .await
                    .map_err(ChangePlanError::Repository)?;
                let mut done = c;
                done.status = SubscriptionPlanChangeStatus::Completed;
                done.completed_at = Some(Utc::now());
                self.changes
                    .update(&done)
                    .await
                    .map_err(ChangePlanError::Repository)?;
                Ok(ChangePlanOutcome {
                    plan_change_id: cid,
                    calculated_amount: s.amount.unwrap_or(Decimal::ZERO),
                    checkout_session_id: Some(session_id),
                    checkout_url: None,
                    applied: true,
                })
            }
        }
    }

    async fn apply(
        &self,
        sub: &domain::Subscription,
        new_plan: PlanTier,
        end: DateTime<Utc>,
        pricing_version: u32,
    ) -> Result<(), ChangePlanError> {
        let mut u = sub.clone();
        u.apply_verified_payment(new_plan)
            .map_err(|e| ChangePlanError::Invalid(e.to_string()))?;
        u.current_period_ends_at = Some(end);
        let p = Plan::for_tier(new_plan);
        u.update_usage(
            p.max_monitors as u64,
            p.min_check_interval_seconds,
            p.history_retention_days,
            pricing_version,
        )
        .map_err(|e| ChangePlanError::Invalid(e.to_string()))?;
        let prev = self
            .periods
            .find_open_period(sub.id)
            .await
            .map_err(ChangePlanError::Repository)?;
        let period = SubscriptionUsagePeriod::open(
            self.ids.next_subscription_usage_period_id().await,
            sub.id,
            p.max_monitors as u64,
            p.min_check_interval_seconds,
            p.history_retention_days,
            Utc::now(),
        );
        self.tx
            .change_usage(&u, prev.map(|x| x.id), &period, Some(sub.plan_tier))
            .await
            .map_err(ChangePlanError::Repository)
    }
}

/// Helper retained so legacy callers that pass `Money` through this path
/// can still refer to the domain type without importing `domain::money`.
#[allow(dead_code)]
fn _money_marker(_m: Money) {}

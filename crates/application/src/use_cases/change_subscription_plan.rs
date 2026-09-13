use crate::ports::*;
use chrono::{Duration, Utc};
use domain::proration::calculate_proration;
use domain::{
    CheckoutSession, Plan, PlanTier, SubscriptionPlanChange, SubscriptionPlanChangeStatus,
    SubscriptionStatus, SubscriptionUsagePeriod,
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;
#[derive(Debug)]
pub enum ChangePlanError {
    SubscriptionNotFound,
    SamePlan,
    Invalid(String),
    Pricing(Box<dyn Error + Send + Sync>),
    Provider(Box<dyn Error + Send + Sync>),
    Repository(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for ChangePlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriptionNotFound => write!(f, "subscription not found"),
            Self::SamePlan => write!(f, "requested plan is already active"),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Pricing(e) | Self::Provider(e) | Self::Repository(e) => write!(f, "{e}"),
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
    pricing: Arc<dyn PricingConfigRepository>,
    changes: Arc<dyn SubscriptionPlanChangeRepository>,
    sessions: Arc<dyn CheckoutSessionRepository>,
    provider: Arc<dyn PaymentProvider>,
    wallet: Arc<dyn WalletRepository>,
    periods: Arc<dyn SubscriptionUsagePeriodRepository>,
    tx: T,
    ids: Arc<dyn IdGenerator>,
}
impl<T: SubscriptionUsageTransaction> ChangeSubscriptionPlanUseCase<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subs: Arc<dyn SubscriptionRepository>,
        pricing: Arc<dyn PricingConfigRepository>,
        changes: Arc<dyn SubscriptionPlanChangeRepository>,
        sessions: Arc<dyn CheckoutSessionRepository>,
        provider: Arc<dyn PaymentProvider>,
        wallet: Arc<dyn WalletRepository>,
        periods: Arc<dyn SubscriptionUsagePeriodRepository>,
        tx: T,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            subs,
            pricing,
            changes,
            sessions,
            provider,
            wallet,
            periods,
            tx,
            ids,
        }
    }
    pub async fn execute(
        &self,
        tenant: domain::shared::ids::TenantId,
        new_plan: PlanTier,
    ) -> Result<ChangePlanOutcome, ChangePlanError> {
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
        let cfg = self
            .pricing
            .find_active_config()
            .await
            .map_err(ChangePlanError::Repository)?
            .ok_or_else(|| ChangePlanError::Invalid("no active pricing config".into()))?;
        let end = sub
            .current_period_ends_at
            .unwrap_or(sub.created_at + Duration::days(30));
        let start = end - Duration::days(30);
        let r = calculate_proration(sub.plan_tier, new_plan, start, end, Utc::now(), &cfg)
            .map_err(|e| ChangePlanError::Pricing(Box::new(e)))?;
        let mut c = SubscriptionPlanChange {
            id: Uuid::new_v4(),
            subscription_id: sub.id.0,
            tenant_id: tenant.0,
            old_plan: r.old_plan,
            new_plan: r.new_plan,
            period_start: r.period_start,
            period_end: r.period_end,
            calculated_amount: r.calculated_amount,
            currency: cfg.currency.clone(),
            pricing_version: r.pricing_version,
            status: SubscriptionPlanChangeStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
        };
        self.changes
            .create(&c)
            .await
            .map_err(ChangePlanError::Repository)?;
        if r.calculated_amount > Decimal::ZERO {
            let cents = (r.calculated_amount * Decimal::from(100))
                .round()
                .to_i64()
                .ok_or_else(|| ChangePlanError::Invalid("proration amount out of range".into()))?;
            let co = self
                .provider
                .create_checkout(&CheckoutRequest {
                    tenant_id: tenant,
                    plan_tier: Some(new_plan),
                    amount_minor: cents,
                    currency: cfg.currency.clone(),
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
            s.attach_plan_change(c.id, r.calculated_amount, cfg.currency.clone());
            self.sessions
                .create(&s)
                .await
                .map_err(ChangePlanError::Repository)?;
            return Ok(ChangePlanOutcome {
                plan_change_id: c.id,
                calculated_amount: r.calculated_amount,
                checkout_session_id: Some(s.id.0),
                checkout_url: Some(co.checkout_url),
                applied: false,
            });
        }
        if r.calculated_amount < Decimal::ZERO {
            let cents = (-r.calculated_amount * Decimal::from(100))
                .round()
                .to_i64()
                .ok_or_else(|| ChangePlanError::Invalid("credit amount out of range".into()))?;
            self.wallet
                .credit(
                    tenant.0,
                    cents,
                    domain::WalletTransactionType::PromotionalCredit,
                    &format!("plan-change:{}", c.id),
                    Some("subscription downgrade proration credit"),
                )
                .await
                .map_err(ChangePlanError::Repository)?;
        }
        self.apply(&sub, new_plan, end, cfg.version).await?;
        c.status = SubscriptionPlanChangeStatus::Completed;
        c.completed_at = Some(Utc::now());
        self.changes
            .update(&c)
            .await
            .map_err(ChangePlanError::Repository)?;
        Ok(ChangePlanOutcome {
            plan_change_id: c.id,
            calculated_amount: r.calculated_amount,
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
        end: chrono::DateTime<Utc>,
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

use crate::ports::*;
use domain::CheckoutStatus;
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;
#[derive(Debug)]
pub enum VerifyAddOnError {
    NotFound,
    Invalid(String),
    Provider(Box<dyn Error + Send + Sync>),
    Repository(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for VerifyAddOnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "add-on checkout not found"),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Provider(e) | Self::Repository(e) => write!(f, "{e}"),
        }
    }
}
impl Error for VerifyAddOnError {}
pub struct VerifyAddOnCheckoutUseCase {
    sessions: Arc<dyn CheckoutSessionRepository>,
    addons: Arc<dyn AddOnSubscriptionRepository>,
    provider: Arc<dyn PaymentProvider>,
}
impl VerifyAddOnCheckoutUseCase {
    pub fn new(
        s: Arc<dyn CheckoutSessionRepository>,
        a: Arc<dyn AddOnSubscriptionRepository>,
        p: Arc<dyn PaymentProvider>,
    ) -> Self {
        Self {
            sessions: s,
            addons: a,
            provider: p,
        }
    }
    pub async fn execute(
        &self,
        tenant: Uuid,
        session_id: Uuid,
    ) -> Result<(Uuid, CheckoutStatus), VerifyAddOnError> {
        let s = self
            .sessions
            .find_by_id(domain::CheckoutSessionId(session_id))
            .await
            .map_err(VerifyAddOnError::Repository)?
            .ok_or(VerifyAddOnError::NotFound)?;
        let id = s.addon_subscription_id.ok_or(VerifyAddOnError::NotFound)?;
        if s.tenant_id.0 != tenant {
            return Err(VerifyAddOnError::NotFound);
        }
        if s.status == CheckoutStatus::Completed {
            return Ok((id, CheckoutStatus::Completed));
        }
        let pid = s
            .provider_checkout_id
            .as_deref()
            .ok_or_else(|| VerifyAddOnError::Invalid("checkout is not ready".into()))?;
        match self
            .provider
            .verify_payment(pid)
            .await
            .map_err(VerifyAddOnError::Provider)?
        {
            PaymentVerification::Paid => {
                let mut a = self
                    .addons
                    .find_by_id(domain::AddOnSubscriptionId(id))
                    .await
                    .map_err(VerifyAddOnError::Repository)?
                    .ok_or(VerifyAddOnError::NotFound)?;
                if a.tenant_id.0 != tenant {
                    return Err(VerifyAddOnError::NotFound);
                }
                a.activate_after_verified_payment()
                    .map_err(VerifyAddOnError::Invalid)?;
                self.addons
                    .update(&a)
                    .await
                    .map_err(VerifyAddOnError::Repository)?;
                let mut d = s.clone();
                d.mark_completed();
                self.sessions
                    .update(&d)
                    .await
                    .map_err(VerifyAddOnError::Repository)?;
                Ok((id, CheckoutStatus::Completed))
            }
            PaymentVerification::Pending => Ok((id, CheckoutStatus::Pending)),
            PaymentVerification::Failed => {
                let mut a = self
                    .addons
                    .find_by_id(domain::AddOnSubscriptionId(id))
                    .await
                    .map_err(VerifyAddOnError::Repository)?
                    .ok_or(VerifyAddOnError::NotFound)?;
                a.mark_failed().map_err(VerifyAddOnError::Invalid)?;
                self.addons
                    .update(&a)
                    .await
                    .map_err(VerifyAddOnError::Repository)?;
                let mut d = s.clone();
                d.mark_failed();
                self.sessions
                    .update(&d)
                    .await
                    .map_err(VerifyAddOnError::Repository)?;
                Ok((id, CheckoutStatus::Failed))
            }
        }
    }
}

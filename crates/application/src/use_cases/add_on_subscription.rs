use crate::ports::AddOnSubscriptionRepository;
use domain::entities::{AddOnStatus, AddOnSubscription, AddOnType};
use domain::shared::ids::{AddOnSubscriptionId, TenantId};
use rust_decimal::Decimal;
use std::sync::Arc;
pub struct CreateAddOnSubscriptionUseCase {
    repo: Arc<dyn AddOnSubscriptionRepository>,
}
impl CreateAddOnSubscriptionUseCase {
    pub fn new(repo: Arc<dyn AddOnSubscriptionRepository>) -> Self {
        Self { repo }
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        tenant_id: TenantId,
        addon_type: AddOnType,
        quantity: u64,
        tier: Option<String>,
        unit_price: Decimal,
        currency: String,
        status: AddOnStatus,
    ) -> Result<AddOnSubscription, Box<dyn std::error::Error + Send + Sync>> {
        let a = AddOnSubscription::new(
            AddOnSubscriptionId::new(),
            tenant_id,
            addon_type,
            quantity,
            tier,
            unit_price,
            currency,
            status,
        )
        .map_err(std::io::Error::other)?;
        self.repo.create(&a).await?;
        Ok(a)
    }
}

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::TenantRepository;
use async_trait::async_trait;
use domain::Tenant;
use domain::shared::ids::{TenantId, UserId};

/// Test-only in-memory stand-in for `TenantRepository`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTenantRepository {
    tenants: Arc<Mutex<HashMap<TenantId, Tenant>>>,
}

impl InMemoryTenantRepository {
    pub fn new() -> Self {
        Self {
            tenants: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn create(&self, tenant: &Tenant) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.tenants
            .lock()
            .unwrap()
            .insert(tenant.id, tenant.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: TenantId,
    ) -> Result<Option<Tenant>, Box<dyn Error + Send + Sync>> {
        Ok(self.tenants.lock().unwrap().get(&id).cloned())
    }

    async fn find_by_owner_user_id(
        &self,
        owner_user_id: UserId,
    ) -> Result<Option<Tenant>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .tenants
            .lock()
            .unwrap()
            .values()
            .find(|tenant| tenant.owner_user_id == owner_user_id)
            .cloned())
    }

    async fn list_all(&self) -> Result<Vec<Tenant>, Box<dyn Error + Send + Sync>> {
        Ok(self.tenants.lock().unwrap().values().cloned().collect())
    }
}

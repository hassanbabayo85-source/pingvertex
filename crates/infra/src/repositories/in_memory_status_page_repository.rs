use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::StatusPageRepository;
use async_trait::async_trait;
use domain::entities::StatusPage;
use domain::shared::ids::{StatusPageId, TenantId, UserId};

/// Simple in-memory repository for development and tests.
#[derive(Debug, Clone, Default)]
pub struct InMemoryStatusPageRepository {
    status_pages: Arc<RwLock<HashMap<StatusPageId, StatusPage>>>,
}

impl InMemoryStatusPageRepository {
    pub fn new() -> Self {
        Self {
            status_pages: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl StatusPageRepository for InMemoryStatusPageRepository {
    async fn save(
        &self,
        status_page: &StatusPage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.status_pages
            .write()
            .unwrap()
            .insert(status_page.id, status_page.clone());
        Ok(())
    }

    async fn update(
        &self,
        status_page: &StatusPage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.status_pages
            .write()
            .unwrap()
            .insert(status_page.id, status_page.clone());
        Ok(())
    }

    async fn delete_for_user(
        &self,
        id: StatusPageId,
        user_id: UserId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut status_pages = self.status_pages.write().unwrap();
        let belongs_to_user = status_pages
            .get(&id)
            .map(|status_page| status_page.user_id == user_id)
            .unwrap_or(false);
        if belongs_to_user {
            Ok(status_pages.remove(&id).is_some())
        } else {
            Ok(false)
        }
    }

    async fn find_by_id_for_user(
        &self,
        id: StatusPageId,
        user_id: UserId,
    ) -> Result<Option<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .status_pages
            .read()
            .unwrap()
            .get(&id)
            .filter(|status_page| status_page.user_id == user_id)
            .cloned())
    }

    async fn find_all_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .status_pages
            .read()
            .unwrap()
            .values()
            .filter(|status_page| status_page.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn find_by_id(
        &self,
        id: StatusPageId,
    ) -> Result<Option<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.status_pages.read().unwrap().get(&id).cloned())
    }

    async fn delete(
        &self,
        id: StatusPageId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.status_pages.write().unwrap().remove(&id).is_some())
    }

    async fn find_all_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .status_pages
            .read()
            .unwrap()
            .values()
            .filter(|status_page| status_page.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn find_by_subdomain(
        &self,
        subdomain: &str,
    ) -> Result<Option<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .status_pages
            .read()
            .unwrap()
            .values()
            .find(|status_page| status_page.subdomain.as_str() == subdomain)
            .cloned())
    }

    async fn find_by_verified_custom_domain(
        &self,
        custom_domain: &str,
    ) -> Result<Option<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .status_pages
            .read()
            .unwrap()
            .values()
            .find(|status_page| status_page.verified_custom_domain() == Some(custom_domain))
            .cloned())
    }

    async fn find_pending_custom_domains(
        &self,
    ) -> Result<Vec<StatusPage>, Box<dyn std::error::Error + Send + Sync>> {
        use domain::entities::CustomDomainVerification;

        Ok(self
            .status_pages
            .read()
            .unwrap()
            .values()
            .filter(|status_page| {
                matches!(
                    &status_page.custom_domain,
                    Some((_, CustomDomainVerification::Pending { .. }))
                )
            })
            .cloned()
            .collect())
    }
}

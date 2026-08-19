#!/usr/bin/env bash
# Patch script — gyaran IDOR a status pages (mai amfani na iya bayyana
# monitor na wani ga jama'a ta hanyar public status page da ba shi ya mallaka ba).
# Run from the ROOT of your sashi project.
set -euo pipefail

mkdir -p "crates/application/src/use_cases"
cat > "crates/application/src/use_cases/create_status_page.rs" << 'SASHI_EOF_MARKER_0'
use std::{error::Error, fmt};

use crate::{
    dto::CreateStatusPageRequest,
    ports::{IdGenerator, MonitorRepository, StatusPageRepository},
};
use domain::{
    MonitorId, Subdomain,
    entities::{CreateStatusPage, StatusPage},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard cap on the number of status pages a single user may create.
///
/// Each status page carries its own polling (public endpoint traffic),
/// cache entries, and — once a custom domain is attached — a recurring
/// background DNS verification check. Without a limit a single account
/// could create an unbounded number of these, each with its own ongoing
/// cost. 3 comfortably covers legitimate use (most teams need one, maybe a
/// couple for different products) while blocking abuse.
pub const MAX_STATUS_PAGES_PER_USER: usize = 3;

#[derive(Debug)]
pub enum CreateStatusPageError {
    QuotaExceeded { limit: usize },
    SubdomainTaken { subdomain: String },
    /// A `monitor_id` in the request doesn't exist, or exists but isn't
    /// owned by the requesting user.
    ///
    /// SECURITY: this check exists to close an IDOR — without it, any
    /// authenticated user could attach ANY monitor id (belonging to any
    /// other user) to their own status page, and the public,
    /// unauthenticated `/api/v1/public/status/{subdomain}` endpoint would
    /// then happily render that other user's monitor name, URL, uptime,
    /// latency, and incident history to the world. See
    /// `render_public_status_page`, which fetches each listed monitor by
    /// bare id with no ownership check of its own — this use case is the
    /// only place in the whole write path where ownership can be
    /// enforced before an id is ever persisted onto a status page.
    MonitorNotFound { monitor_id: MonitorId },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "status page limit reached: at most {limit} status pages are allowed per account"
                )
            }
            Self::SubdomainTaken { subdomain } => {
                write!(f, "subdomain \"{subdomain}\" is already taken")
            }
            Self::MonitorNotFound { monitor_id } => {
                write!(f, "monitor {monitor_id} not found or access denied")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateStatusPageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::QuotaExceeded { .. } => None,
            Self::SubdomainTaken { .. } => None,
            Self::MonitorNotFound { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateStatusPageUseCase<R, G, M> {
    repository: R,
    id_generator: G,
    monitor_repository: M,
}

impl<R, G, M> CreateStatusPageUseCase<R, G, M>
where
    R: StatusPageRepository,
    G: IdGenerator,
    M: MonitorRepository,
{
    pub fn new(repository: R, id_generator: G, monitor_repository: M) -> Self {
        Self {
            repository,
            id_generator,
            monitor_repository,
        }
    }

    pub async fn execute(
        &self,
        request: CreateStatusPageRequest,
    ) -> Result<StatusPage, CreateStatusPageError> {
        // Validate the subdomain shape first — cheap, in-memory, no
        // database round-trip — before doing any repository work.
        let subdomain = Subdomain::new(request.subdomain.clone())
            .map_err(|err| CreateStatusPageError::Other(Box::new(err)))?;

        let existing_pages = self
            .repository
            .find_all_for_user(request.user_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        if existing_pages.len() >= MAX_STATUS_PAGES_PER_USER {
            return Err(CreateStatusPageError::QuotaExceeded {
                limit: MAX_STATUS_PAGES_PER_USER,
            });
        }

        // Friendly pre-check before hitting the DB unique-index collision.
        // Still relies on the DB constraint as the real source of truth
        // (this check has an inherent TOCTOU race under concurrent
        // requests for the same subdomain), but it turns the common case
        // into a clean, specific error instead of a generic DB error.
        if self
            .repository
            .find_by_subdomain(subdomain.as_str())
            .await
            .map_err(CreateStatusPageError::Other)?
            .is_some()
        {
            return Err(CreateStatusPageError::SubdomainTaken {
                subdomain: subdomain.as_str().to_owned(),
            });
        }

        // SECURITY: every monitor_id listed must belong to this user
        // before it is ever attached to a status page — see
        // `CreateStatusPageError::MonitorNotFound` for why.
        self.assert_monitors_owned(request.user_id, &request.monitor_ids)
            .await?;

        self.build_and_save(request, subdomain)
            .await
            .map_err(CreateStatusPageError::Other)
    }

    async fn assert_monitors_owned(
        &self,
        user_id: domain::UserId,
        monitor_ids: &[MonitorId],
    ) -> Result<(), CreateStatusPageError> {
        for &monitor_id in monitor_ids {
            self.monitor_repository
                .find_by_id_for_user(monitor_id, user_id)
                .await
                .map_err(CreateStatusPageError::Other)?
                .ok_or(CreateStatusPageError::MonitorNotFound { monitor_id })?;
        }

        Ok(())
    }

    async fn build_and_save(
        &self,
        request: CreateStatusPageRequest,
        subdomain: Subdomain,
    ) -> UseCaseResult<StatusPage> {
        let id = self.id_generator.next_status_page_id().await;

        let status_page = StatusPage::create(CreateStatusPage {
            id,
            user_id: request.user_id,
            name: request.name,
            subdomain,
            monitor_ids: request.monitor_ids,
        })?;

        self.repository.save(&status_page).await?;

        Ok(status_page)
    }
}

SASHI_EOF_MARKER_0
echo "wrote crates/application/src/use_cases/create_status_page.rs"

mkdir -p "crates/application/src/use_cases"
cat > "crates/application/src/use_cases/update_status_page.rs" << 'SASHI_EOF_MARKER_1'
use std::{error::Error, fmt};

use crate::{
    commands::UpdateStatusPage,
    ports::{MonitorRepository, StatusPageRepository},
};
use domain::{HexColor, MonitorId, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `UpdateStatusPageUseCase`.
#[derive(Debug)]
pub enum UpdateStatusPageError {
    /// A `monitor_id` in the request doesn't exist, or exists but isn't
    /// owned by the requesting user.
    ///
    /// SECURITY: mirrors `CreateStatusPageError::MonitorNotFound` — this
    /// is the update-path half of closing the same IDOR. Without this
    /// check, a user could edit an EXISTING status page they legitimately
    /// own and add another user's monitor id to it after the fact, which
    /// would then be exposed on the public, unauthenticated status page
    /// endpoint exactly like at creation time.
    MonitorNotFound { monitor_id: MonitorId },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for UpdateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MonitorNotFound { monitor_id } => {
                write!(f, "monitor {monitor_id} not found or access denied")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UpdateStatusPageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MonitorNotFound { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

impl From<Box<dyn Error + Send + Sync>> for UpdateStatusPageError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Other(err)
    }
}

impl From<domain::errors::DomainError> for UpdateStatusPageError {
    fn from(err: domain::errors::DomainError) -> Self {
        Self::Other(Box::new(err))
    }
}

pub struct UpdateStatusPageUseCase<R, M> {
    repository: R,
    monitor_repository: M,
}

impl<R, M> UpdateStatusPageUseCase<R, M>
where
    R: StatusPageRepository,
    M: MonitorRepository,
{
    pub fn new(repository: R, monitor_repository: M) -> Self {
        Self {
            repository,
            monitor_repository,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateStatusPage,
    ) -> Result<Option<StatusPage>, UpdateStatusPageError> {
        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        // SECURITY: every monitor_id listed must belong to this user
        // before it replaces the status page's monitor set — see
        // `UpdateStatusPageError::MonitorNotFound` for why.
        self.assert_monitors_owned(command.user_id, &command.monitor_ids)
            .await?;

        status_page.rename(command.name)?;
        status_page.set_monitors(command.monitor_ids)?;
        status_page.set_logo_url(command.logo_url)?;

        let primary_color = command.primary_color.map(HexColor::new).transpose()?;
        status_page.set_primary_color(primary_color);

        self.repository.update(&status_page).await?;

        Ok(Some(status_page))
    }

    async fn assert_monitors_owned(
        &self,
        user_id: domain::UserId,
        monitor_ids: &[MonitorId],
    ) -> Result<(), UpdateStatusPageError> {
        for &monitor_id in monitor_ids {
            self.monitor_repository
                .find_by_id_for_user(monitor_id, user_id)
                .await
                .map_err(UpdateStatusPageError::Other)?
                .ok_or(UpdateStatusPageError::MonitorNotFound { monitor_id })?;
        }

        Ok(())
    }
}
SASHI_EOF_MARKER_1
echo "wrote crates/application/src/use_cases/update_status_page.rs"

mkdir -p "crates/application/src/use_cases"
cat > "crates/application/src/use_cases/mod.rs" << 'SASHI_EOF_MARKER_2'
pub mod acknowledge_incident;
pub mod create_api_flow;
pub mod create_escalation_policy;
pub mod create_maintenance_window;
pub mod create_monitor;
pub mod create_notification_channel;
pub mod delete_api_flow;
pub mod delete_monitor;
pub mod delete_notification_channel;
pub mod get_monitor;
pub mod list_checks;
pub mod list_incidents;
pub mod list_monitor_checks;
pub mod list_monitors;
pub mod list_notification_channels;
pub mod register_user;
pub mod send_test_notification;
pub mod update_api_flow;
pub mod update_monitor;
pub use acknowledge_incident::{AcknowledgeIncidentError, AcknowledgeIncidentUseCase};
pub use create_api_flow::{CreateApiFlowError, CreateApiFlowUseCase};
pub use create_escalation_policy::{CreateEscalationPolicyError, CreateEscalationPolicyUseCase};
pub use create_maintenance_window::{CreateMaintenanceWindowError, CreateMaintenanceWindowUseCase};
pub use create_monitor::CreateMonitorUseCase;
pub use create_notification_channel::{
    CreateNotificationChannelError, CreateNotificationChannelUseCase,
};
pub use delete_api_flow::DeleteApiFlowUseCase;
pub use delete_monitor::DeleteMonitorUseCase;
pub use delete_notification_channel::DeleteNotificationChannelUseCase;
pub use get_monitor::GetMonitorUseCase;
pub use list_checks::ListChecksUseCase;
pub use list_incidents::{ListIncidentsError, ListIncidentsUseCase};
pub use list_monitor_checks::{ListMonitorChecksError, ListMonitorChecksUseCase};
pub use list_monitors::ListMonitorsUseCase;
pub use list_notification_channels::{
    ListNotificationChannelsError, ListNotificationChannelsUseCase,
};
pub use register_user::RegisterUserUseCase;
pub use send_test_notification::{SendTestNotificationError, SendTestNotificationUseCase};
pub use update_api_flow::UpdateApiFlowUseCase;
pub use update_monitor::UpdateMonitorUseCase;
mod login_user;
pub use login_user::*;
pub mod create_status_page;
pub mod delete_status_page;
pub mod list_status_pages;
pub mod request_custom_domain;
pub mod update_status_page;
pub use create_status_page::{CreateStatusPageError, CreateStatusPageUseCase};
pub use delete_status_page::DeleteStatusPageUseCase;
pub use list_status_pages::ListStatusPagesUseCase;
pub use request_custom_domain::RequestCustomDomainUseCase;
pub use update_status_page::{UpdateStatusPageError, UpdateStatusPageUseCase};
pub mod upload_status_page_logo;
pub use upload_status_page_logo::{UploadStatusPageLogoError, UploadStatusPageLogoUseCase};
pub mod submit_proxy_check_job;
pub use submit_proxy_check_job::{SubmitProxyCheckJobError, SubmitProxyCheckJobUseCase};
pub mod get_proxy_check_job;
pub use get_proxy_check_job::GetProxyCheckJobUseCase;
SASHI_EOF_MARKER_2
echo "wrote crates/application/src/use_cases/mod.rs"

mkdir -p "crates/api/src/handlers"
cat > "crates/api/src/handlers/create_status_page.rs" << 'SASHI_EOF_MARKER_3'
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    dto::CreateStatusPageRequest,
    use_cases::{CreateStatusPageError, CreateStatusPageUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageCreateRequest, StatusPageResponse},
    state::AppState,
    validation::parse_monitor_id,
};

/// Creates a status page through the API.
///
/// Every `monitor_id` in the request is verified to belong to the
/// authenticated user (see `CreateStatusPageUseCase::assert_monitors_owned`)
/// — this status page's public, unauthenticated endpoint will otherwise
/// serve whatever monitor is listed here to anyone, regardless of who
/// owns it.
pub async fn create_status_page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Json(payload): Json<StatusPageCreateRequest>,
) -> Result<(StatusCode, Json<StatusPageResponse>), ApiError> {
    info!("API: create_status_page");

    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|id| parse_monitor_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let request = CreateStatusPageRequest {
        user_id,
        name: payload.name,
        subdomain: payload.subdomain,
        monitor_ids,
    };

    let use_case = CreateStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.id_generator,
        state.monitor_repository.clone(),
    );

    let status_page = use_case.execute(request).await.map_err(|err| match err {
        CreateStatusPageError::QuotaExceeded { .. }
        | CreateStatusPageError::SubdomainTaken { .. } => ApiError::conflict(err.to_string()),
        CreateStatusPageError::MonitorNotFound { .. } => ApiError::not_found(err.to_string()),
        CreateStatusPageError::Other(inner) => {
            ApiError::internal_with_source("failed to create status page", inner)
        }
    })?;

    Ok((
        StatusCode::CREATED,
        Json(StatusPageResponse::from(status_page)),
    ))
}
SASHI_EOF_MARKER_3
echo "wrote crates/api/src/handlers/create_status_page.rs"

mkdir -p "crates/api/src/handlers"
cat > "crates/api/src/handlers/update_status_page.rs" << 'SASHI_EOF_MARKER_4'
use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{
    commands::UpdateStatusPage,
    use_cases::{UpdateStatusPageError, UpdateStatusPageUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageResponse, StatusPageUpdateRequest},
    state::AppState,
    validation::{parse_monitor_id, parse_status_page_id},
};

/// Updates a status page's name, monitor selection, logo, and brand color.
///
/// Every `monitor_id` in the request is re-verified to belong to the
/// authenticated user on every update, not just at creation — see
/// `UpdateStatusPageUseCase::assert_monitors_owned`.
pub async fn update_status_page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
    Json(payload): Json<StatusPageUpdateRequest>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: update_status_page");

    let id = parse_status_page_id(&id)?;

    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|mid| parse_monitor_id(mid))
        .collect::<Result<Vec<_>, _>>()?;

    let command = UpdateStatusPage {
        id,
        user_id,
        name: payload.name,
        monitor_ids,
        logo_url: payload.logo_url,
        primary_color: payload.primary_color,
    };

    let use_case = UpdateStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.monitor_repository.clone(),
    );

    let status_page = use_case
        .execute(command)
        .await
        .map_err(|err| match err {
            UpdateStatusPageError::MonitorNotFound { .. } => ApiError::not_found(err.to_string()),
            UpdateStatusPageError::Other(inner) => {
                ApiError::internal_with_source("failed to update status page", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
SASHI_EOF_MARKER_4
echo "wrote crates/api/src/handlers/update_status_page.rs"

mkdir -p "crates/application/tests"
cat > "crates/application/tests/create_status_page_use_case.rs" << 'SASHI_EOF_MARKER_5'
use application::dto::CreateStatusPageRequest;
use application::ports::MonitorRepository;
use application::use_cases::create_status_page::MAX_STATUS_PAGES_PER_USER;
use application::use_cases::{CreateStatusPageError, CreateStatusPageUseCase};
use domain::shared::ids::{MonitorId, UserId};
use domain::{
    ExpectedStatus, HttpMethod, Monitor, MonitorBuilder, MonitorName, CheckInterval, Timeout, Url,
};
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

fn request(user_id: UserId, subdomain: &str) -> CreateStatusPageRequest {
    CreateStatusPageRequest {
        user_id,
        name: "Acme Status".into(),
        subdomain: subdomain.into(),
        monitor_ids: vec![],
    }
}

fn use_case(
    status_pages: InMemoryStatusPageRepository,
    monitors: InMemoryMonitorRepository,
) -> CreateStatusPageUseCase<InMemoryStatusPageRepository, UuidIdGenerator, InMemoryMonitorRepository>
{
    CreateStatusPageUseCase::new(status_pages, UuidIdGenerator::new(), monitors)
}

#[tokio::test]
async fn create_status_page_persists_status_page() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());

    let status_page = use_case.execute(request(user_id, "acme")).await.unwrap();

    assert_eq!(status_page.name, "Acme Status");
    assert_eq!(status_page.subdomain.as_str(), "acme");
}

#[tokio::test]
async fn rejects_duplicate_subdomain() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    use_case.execute(request(user_id, "acme")).await.unwrap();

    // Even a different user cannot take an already-used subdomain —
    // subdomains are globally unique, not per-user.
    let result = use_case.execute(request(other_user_id, "acme")).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn rejects_creation_past_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());

    for i in 0..MAX_STATUS_PAGES_PER_USER {
        use_case
            .execute(request(user_id, &format!("acme-{i}")))
            .await
            .unwrap();
    }

    let result = use_case.execute(request(user_id, "one-too-many")).await;

    assert!(result.is_err());
}

fn sample_monitor(user_id: UserId) -> Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id)
        .name(MonitorName::new("Acme API").unwrap())
        .url(Url::new("https://api.acme.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

// SECURITY REGRESSION TEST — this is the exact bug that was found and
// fixed: a status page must never be creatable with a monitor_id that
// belongs to a different user, because the public, unauthenticated status
// page endpoint would otherwise expose that other user's monitor (name,
// URL, uptime, latency, incidents) to anyone.
#[tokio::test]
async fn rejects_monitor_not_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();

    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let victim_monitor = sample_monitor(owner_id);
    monitors.save(&victim_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);

    let result = use_case
        .execute(CreateStatusPageRequest {
            user_id: attacker_id,
            name: "Attacker Status".into(),
            subdomain: "attacker".into(),
            monitor_ids: vec![victim_monitor.id],
        })
        .await;

    assert!(matches!(
        result,
        Err(CreateStatusPageError::MonitorNotFound { .. })
    ));
}

#[tokio::test]
async fn allows_monitor_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();

    let owner_id = UserId(Uuid::new_v4());
    let own_monitor = sample_monitor(owner_id);
    monitors.save(&own_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);

    let status_page = use_case
        .execute(CreateStatusPageRequest {
            user_id: owner_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![own_monitor.id],
        })
        .await
        .unwrap();

    assert_eq!(status_page.monitor_ids, vec![own_monitor.id]);
}
SASHI_EOF_MARKER_5
echo "wrote crates/application/tests/create_status_page_use_case.rs"

mkdir -p "crates/application/tests"
cat > "crates/application/tests/update_status_page_use_case.rs" << 'SASHI_EOF_MARKER_6'
use application::commands::UpdateStatusPage;
use application::dto::CreateStatusPageRequest;
use application::ports::MonitorRepository;
use application::use_cases::{
    CreateStatusPageUseCase, UpdateStatusPageError, UpdateStatusPageUseCase,
};
use domain::shared::ids::{MonitorId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, Monitor, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

fn sample_monitor(user_id: UserId) -> Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id)
        .name(MonitorName::new("Acme API").unwrap())
        .url(Url::new("https://api.acme.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

#[tokio::test]
async fn updates_status_page_successfully() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let user_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let update_use_case = UpdateStatusPageUseCase::new(repository.clone(), monitors);

    let updated = update_use_case
        .execute(UpdateStatusPage {
            id: status_page.id,
            user_id,
            name: "Acme Status Page".into(),
            monitor_ids: vec![],
            logo_url: Some("https://cdn.example.com/logo.png".into()),
            primary_color: Some("#336699".into()),
        })
        .await
        .unwrap()
        .expect("status page should exist");

    assert_eq!(updated.name, "Acme Status Page");
    assert_eq!(
        updated.logo_url.as_deref(),
        Some("https://cdn.example.com/logo.png")
    );
    assert!(updated.primary_color.is_some());
}

#[tokio::test]
async fn cannot_update_status_page_owned_by_another_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id: owner_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let update_use_case = UpdateStatusPageUseCase::new(repository.clone(), monitors);

    let result = update_use_case
        .execute(UpdateStatusPage {
            id: status_page.id,
            user_id: attacker_id,
            name: "Hijacked".into(),
            monitor_ids: vec![],
            logo_url: None,
            primary_color: None,
        })
        .await
        .unwrap();

    assert!(result.is_none());
}

// SECURITY REGRESSION TEST — the update-path half of the same IDOR fixed
// on the create path: editing an existing, legitimately-owned status page
// must not be a way to attach another user's monitor to it after the
// fact.
#[tokio::test]
async fn rejects_adding_a_monitor_not_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();

    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
    );

    // Attacker legitimately owns their own status page...
    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id: attacker_id,
            name: "Attacker Status".into(),
            subdomain: "attacker".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    // ...but tries to attach the victim's monitor to it via update.
    let victim_monitor = sample_monitor(owner_id);
    monitors.save(&victim_monitor).await.unwrap();

    let update_use_case = UpdateStatusPageUseCase::new(repository, monitors);

    let result = update_use_case
        .execute(UpdateStatusPage {
            id: status_page.id,
            user_id: attacker_id,
            name: "Attacker Status".into(),
            monitor_ids: vec![victim_monitor.id],
            logo_url: None,
            primary_color: None,
        })
        .await;

    assert!(matches!(
        result,
        Err(UpdateStatusPageError::MonitorNotFound { .. })
    ));
}
SASHI_EOF_MARKER_6
echo "wrote crates/application/tests/update_status_page_use_case.rs"

mkdir -p "crates/application/tests"
cat > "crates/application/tests/request_custom_domain_use_case.rs" << 'SASHI_EOF_MARKER_7'
use application::commands::RequestCustomDomain;
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, RequestCustomDomainUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn request_custom_domain_starts_in_pending_state() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let user_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let request_domain_use_case = RequestCustomDomainUseCase::new(repository.clone());

    let updated = request_domain_use_case
        .execute(RequestCustomDomain {
            id: status_page.id,
            user_id,
            domain: "status.acme.com".into(),
        })
        .await
        .unwrap()
        .expect("status page should exist");

    // Freshly requested — NOT verified yet, and therefore must not be
    // usable for host-header routing. This is the security invariant the
    // whole custom-domain feature depends on.
    assert!(updated.verified_custom_domain().is_none());

    let persisted = repository
        .find_by_id_for_user(status_page.id, user_id)
        .await
        .unwrap()
        .expect("status page should exist in repository");

    assert!(persisted.verified_custom_domain().is_none());
    assert!(persisted.custom_domain.is_some());
}

#[tokio::test]
async fn cannot_request_custom_domain_for_someone_elses_status_page() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id: owner_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let request_domain_use_case = RequestCustomDomainUseCase::new(repository.clone());

    let result = request_domain_use_case
        .execute(RequestCustomDomain {
            id: status_page.id,
            user_id: attacker_id,
            domain: "status.evil.com".into(),
        })
        .await
        .unwrap();

    assert!(result.is_none());
}
SASHI_EOF_MARKER_7
echo "wrote crates/application/tests/request_custom_domain_use_case.rs"

mkdir -p "crates/application/tests"
cat > "crates/application/tests/delete_status_page_use_case.rs" << 'SASHI_EOF_MARKER_8'
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, DeleteStatusPageUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn deletes_status_page_successfully() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let user_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone());

    let deleted = delete_use_case
        .execute(status_page.id, user_id)
        .await
        .unwrap();

    assert!(deleted);

    let remaining = repository
        .find_by_id_for_user(status_page.id, user_id)
        .await
        .unwrap();

    assert!(remaining.is_none());
}

#[tokio::test]
async fn deleting_someone_elses_status_page_returns_false() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id: owner_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone());

    let deleted = delete_use_case
        .execute(status_page.id, attacker_id)
        .await
        .unwrap();

    assert!(!deleted);
}
SASHI_EOF_MARKER_8
echo "wrote crates/application/tests/delete_status_page_use_case.rs"

mkdir -p "crates/application/tests"
cat > "crates/application/tests/list_status_pages_use_case.rs" << 'SASHI_EOF_MARKER_9'
use application::dto::CreateStatusPageRequest;
use application::use_cases::{CreateStatusPageUseCase, ListStatusPagesUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn lists_only_status_pages_owned_by_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new(), monitors.clone());
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Internal Status".into(),
            subdomain: "acme-internal".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id: other_user_id,
            name: "Widgets Inc Status".into(),
            subdomain: "widgets".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let list_use_case = ListStatusPagesUseCase::new(repository.clone());

    let pages = list_use_case.execute(user_id).await.unwrap();

    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| page.user_id == user_id));
}
SASHI_EOF_MARKER_9
echo "wrote crates/application/tests/list_status_pages_use_case.rs"

echo "Done. Now run:"
echo "  cargo test -p application --test create_status_page_use_case"
echo "  cargo test -p application --test update_status_page_use_case"
echo "  cargo test -p application --test request_custom_domain_use_case"
echo "  cargo test -p application --test delete_status_page_use_case"
echo "  cargo test -p application --test list_status_pages_use_case"
echo "  cargo build --workspace && cargo test --workspace"

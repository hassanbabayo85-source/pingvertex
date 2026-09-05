use application::use_cases::{GetProxyCheckJobUseCase, SubmitProxyCheckJobUseCase};
use domain::ProxyCheckJob;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::{InMemoryAuditLogEntryRepository, InMemoryProxyCheckJobRepository, UuidIdGenerator};

use application::ports::ProxyCheckJobRepository;
#[tokio::test]
async fn get_proxy_job_is_tenant_scoped() {
    let repo = InMemoryProxyCheckJobRepository::new();
    let owner = UserId::new();
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();
    let job = ProxyCheckJob::new(
        domain::shared::ids::ProxyCheckJobId::new(),
        owner,
        tenant_a,
        0,
    );
    let id = job.id();
    repo.save_job(&job, &[]).await.unwrap();

    let use_case = GetProxyCheckJobUseCase::new(repo);
    let actor_b = AuthenticatedUser::new(owner, Role::Member, TenantContext::new(tenant_b));
    assert!(use_case.execute(id, actor_b).await.unwrap().is_none());
}

#[tokio::test]
async fn create_proxy_job_persists_under_authenticated_tenant() {
    let repo = InMemoryProxyCheckJobRepository::new();
    let audit = std::sync::Arc::new(InMemoryAuditLogEntryRepository::new());
    let actor = AuthenticatedUser::new(
        UserId::new(),
        Role::Member,
        TenantContext::new(TenantId::new()),
    );
    let use_case = SubmitProxyCheckJobUseCase::new(repo.clone(), UuidIdGenerator::new(), audit);

    let request = application::dto::CreateProxyCheckJobRequest {
        entries: vec![application::dto::ProxyEntryInput {
            host: "8.8.8.8".into(),
            port: 8080,
            scheme: domain::ProxyScheme::Http,
            username: None,
            password: None,
        }],
    };
    let job = use_case.execute(request, actor).await.unwrap();
    assert_eq!(job.tenant_id(), actor.tenant_context().tenant_id());
}

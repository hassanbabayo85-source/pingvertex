use application::ports::ProxyCheckJobRepository;
use chrono::Duration;
use domain::shared::ids::{ProxyCheckJobId, TenantId, UserId};
use domain::{ProxyCheckJob, ProxyCheckJobStatus};
use infra::InMemoryProxyCheckJobRepository;
use std::sync::Arc;

#[tokio::test]
async fn concurrent_in_memory_claim_is_exactly_once() {
    let repo = Arc::new(InMemoryProxyCheckJobRepository::new());
    let job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), TenantId::new(), 0);
    let id = job.id();
    repo.save_job(&job, &[]).await.unwrap();

    let a = repo.clone();
    let b = repo.clone();
    let (left, right) = tokio::join!(a.try_mark_running(id, "a"), b.try_mark_running(id, "b"));
    assert_eq!(
        [left.unwrap(), right.unwrap()]
            .into_iter()
            .filter(|x| *x)
            .count(),
        1
    );
}

#[tokio::test]
async fn stale_job_is_requeued_then_eventually_failed() {
    let repo = InMemoryProxyCheckJobRepository::new();
    let job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), TenantId::new(), 0);
    let id = job.id();
    repo.save_job(&job, &[]).await.unwrap();
    assert!(repo.try_mark_running(id, "worker").await.unwrap());

    // First stale recovery returns the job to pending.
    let count = repo
        .recover_stale_jobs(chrono::Utc::now() + Duration::seconds(1), 3)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        repo.find_job_by_id(id, job.tenant_id())
            .await
            .unwrap()
            .unwrap()
            .status(),
        ProxyCheckJobStatus::Pending
    );

    // Claim and recover two more times; the third stale cycle reaches the ceiling.
    for worker in ["w2", "w3", "w4"] {
        let _ = repo.try_mark_running(id, worker).await.unwrap();
        let _ = repo
            .recover_stale_jobs(chrono::Utc::now() + Duration::seconds(1), 3)
            .await
            .unwrap();
    }
    assert_eq!(
        repo.find_job_by_id(id, job.tenant_id())
            .await
            .unwrap()
            .unwrap()
            .status(),
        ProxyCheckJobStatus::Failed
    );
}

#[tokio::test]
async fn cancel_is_tenant_scoped_and_terminal_jobs_cannot_be_canceled() {
    let repo = InMemoryProxyCheckJobRepository::new();
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();
    let job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), tenant_a, 0);
    let id = job.id();
    repo.save_job(&job, &[]).await.unwrap();

    assert!(!repo.cancel_job(id, tenant_b).await.unwrap());
    assert!(repo.cancel_job(id, tenant_a).await.unwrap());
    assert!(!repo.cancel_job(id, tenant_a).await.unwrap());
}

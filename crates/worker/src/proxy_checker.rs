//! Worker-side execution of a `ProxyCheckJob`: checks every `ProxyEntry`
//! belonging to the job concurrently (bounded by a semaphore), persists
//! each entry's result as it finishes, then marks the job completed.
//!
//! SSRF defense: every entry is resolved/validated via
//! `crate::ssrf::resolve_and_validate_destination` (Layer 2 — DNS
//! resolution + private-IP rejection + address pinning) immediately
//! before connecting, exactly as `crate::checker::check_monitor` does for
//! monitor URLs. No new SSRF logic is implemented here; this module only
//! calls the existing shared validator.
//!
//! Scope note (this prompt): `AnonymityLevel` detection and
//! `country_code` (GeoIP) lookup are deliberately deferred to a future
//! prompt — neither a test endpoint for anonymity detection nor a GeoIP
//! data source exists anywhere in this codebase yet, and inventing either
//! here would be guessing at product behavior rather than implementing a
//! specified one. Every `ProxyResult` produced by this module always has
//! `anonymity: None` and `country_code: None`. `supports_https` is
//! likewise always `false` here — a real HTTPS CONNECT-tunnel test is out
//! of scope for this prompt for the same reason; only raw TCP
//! reachability of the proxy's own address is tested.

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::ProxyCheckJobRepository;
use domain::ProxyScheme;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::value_objects::ProxyResult;
use tokio::{net::TcpStream, sync::Semaphore, task::JoinSet, time::timeout};
use tracing::{error, info, warn};

/// Upper bound on how many proxy entries within a single job are checked
/// concurrently. Keeps one bulk job from monopolizing this worker
/// process's connection/file-descriptor budget while other monitors,
/// flows, and jobs belonging to other users are also being processed
/// concurrently in the same process.
const MAX_CONCURRENT_CHECKS_PER_JOB: usize = 20;

/// Per-entry timeout for the TCP connect attempt. A slow/dead proxy must
/// not be allowed to stall a job indefinitely.
const PER_ENTRY_TIMEOUT_SECS: u64 = 10;

fn proxy_scheme_str(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks5 => "socks5",
    }
}

/// Checks a single proxy entry and returns its `ProxyResult`.
///
/// Steps:
/// 1. Build a `scheme://host:port` URL from the entry's own fields and
///    run it through `resolve_and_validate_destination` — this performs
///    the DNS lookup exactly once, rejects any private/reserved resolved
///    address (Layer 2 SSRF defense, on top of `ProxyHost::new`'s
///    literal-IP check already performed at submission time), and
///    returns the pinned `SocketAddr` to connect to.
/// 2. Attempt a raw TCP connect to that pinned address, bounded by
///    `PER_ENTRY_TIMEOUT_SECS`.
///
/// A resolution/validation failure and a connection failure are both
/// reported as `reachable: false` with a descriptive `error`, never as a
/// panic or a propagated `Result` — one bad proxy entry must never stop
/// the rest of the job from being checked.
async fn check_single_entry(entry: &ProxyEntry) -> ProxyResult {
    let target = format!(
        "{}://{}:{}",
        proxy_scheme_str(entry.scheme()),
        entry.host().as_str(),
        entry.port()
    );

    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_destination(&target).await {
        Ok(destination) => destination,
        Err(err) => {
            return ProxyResult {
                reachable: false,
                latency_ms: None,
                anonymity: None,
                supports_https: false,
                country_code: None,
                error: Some(err),
            };
        }
    };

    let connect_result = timeout(
        Duration::from_secs(PER_ENTRY_TIMEOUT_SECS),
        TcpStream::connect(destination.socket_addr),
    )
    .await;

    let latency_ms: u32 = started.elapsed().as_millis().try_into().unwrap_or(u32::MAX);

    match connect_result {
        Ok(Ok(_stream)) => ProxyResult {
            reachable: true,
            latency_ms: Some(latency_ms),
            anonymity: None,
            supports_https: false,
            country_code: None,
            error: None,
        },
        Ok(Err(err)) => ProxyResult {
            reachable: false,
            latency_ms: Some(latency_ms),
            anonymity: None,
            supports_https: false,
            country_code: None,
            error: Some(err.to_string()),
        },
        Err(_) => ProxyResult {
            reachable: false,
            latency_ms: None,
            anonymity: None,
            supports_https: false,
            country_code: None,
            error: Some(format!(
                "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
            )),
        },
    }
}

/// Runs every entry of `job` through `check_single_entry`, persisting
/// each result and the job's overall progress/status as it goes.
///
/// `job` must already have been persisted (via
/// `ProxyCheckJobRepository::save_job`, called by the application layer's
/// submit use case) and `entries` must be exactly the entries belonging
/// to it. This function's only responsibilities are: transition the job
/// to `Running` (an optimistic-lock no-op if some other worker already
/// picked it up — see `ProxyCheckJob::mark_running`'s doc comment), check
/// every entry concurrently (bounded by `MAX_CONCURRENT_CHECKS_PER_JOB`),
/// persist each entry's result via `update_entry`, and finally mark the
/// job `Completed` and persist that.
///
/// Dispatch (how a submitted job reaches this function in the first
/// place — polling, LISTEN/NOTIFY, a channel, etc.) is out of scope here
/// and left to a separate prompt, per the task boundary for this module.
pub async fn run_proxy_check_job<R>(
    repository: &R,
    mut job: ProxyCheckJob,
    entries: Vec<ProxyEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: ProxyCheckJobRepository + ?Sized,
{
    if !job.mark_running() {
        // Another worker already observed this job as non-Pending and
        // transitioned it first; this call is a no-op by design (see
        // `ProxyCheckJob::mark_running`).
        info!(job_id = %job.id(), "proxy check job already running or finished; skipping");
        return Ok(());
    }

    // This state transition is important enough to propagate a failure
    // for: if it can't even be persisted, the job's status in storage
    // would silently disagree with what this worker believes, so the
    // caller should treat this as fatal for this job rather than
    // continuing to check entries against a job the database still shows
    // as `Pending`.
    repository.update_job(&job).await?;

    info!(
        job_id = %job.id(),
        entry_count = entries.len(),
        "starting proxy check job"
    );

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS_PER_JOB));
    let mut tasks = JoinSet::new();

    for entry in entries {
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore is never closed while this job is running");

            let mut entry = entry;
            let result = check_single_entry(&entry).await;
            entry.set_result(result);
            entry
        });
    }

    let mut checked_count: u32 = 0;

    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok(entry) => {
                checked_count += 1;

                if let Err(err) = repository.update_entry(&entry).await {
                    // A single entry failing to persist must not abort
                    // the rest of the job — log and continue, mirroring
                    // how `scheduler.rs` treats a failed `Check` save as
                    // non-fatal to the overall sweep.
                    error!(
                        job_id = %job.id(),
                        entry_id = %entry.id(),
                        error = %err,
                        "failed to persist proxy entry result"
                    );
                }

                job.record_progress(checked_count);

                if let Err(err) = repository.update_job(&job).await {
                    error!(
                        job_id = %job.id(),
                        error = %err,
                        "failed to persist proxy check job progress"
                    );
                }
            }
            Err(join_err) => {
                // The checking task itself panicked. The entry it was
                // checking never got a result recorded, but the job as a
                // whole must still be able to reach `Completed` once
                // every task has resolved (panicked or not) — counting it
                // here keeps `checked_count` converging on
                // `entries.len()` rather than stalling short of it.
                checked_count += 1;
                job.record_progress(checked_count);

                warn!(
                    job_id = %job.id(),
                    error = %join_err,
                    "proxy entry check task panicked"
                );
            }
        }
    }

    job.mark_completed();

    if let Err(err) = repository.update_job(&job).await {
        error!(
            job_id = %job.id(),
            error = %err,
            "failed to persist proxy check job completion"
        );
        return Err(err);
    }

    info!(job_id = %job.id(), "proxy check job completed");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::IdGenerator;
    use domain::shared::ids::UserId;
    use domain::value_objects::ProxyHost;
    use infra::{InMemoryProxyCheckJobRepository, UuidIdGenerator};

    #[tokio::test]
    async fn run_proxy_check_job_marks_job_completed_and_records_entry_result() {
        let repo = InMemoryProxyCheckJobRepository::new();
        let id_generator = UuidIdGenerator::new();

        let user_id = UserId::new();
        let job_id = id_generator.next_proxy_check_job_id().await;
        let job = ProxyCheckJob::new(job_id, user_id, 1).unwrap();

        let entry_id = id_generator.next_proxy_entry_id().await;
        let host = ProxyHost::new("localhost").unwrap();
        let entry = ProxyEntry::new(entry_id, job_id, host, 8080, ProxyScheme::Http, None).unwrap();

        repo.save_job(&job, &[entry]).await.unwrap();

        let entries = repo.find_entries_by_job_id(job_id).await.unwrap();

        run_proxy_check_job(&repo, job.clone(), entries)
            .await
            .unwrap();

        let job_after = repo.find_job_by_id(job_id).await.unwrap().unwrap();
        assert_eq!(job_after.status(), &domain::ProxyCheckJobStatus::Completed);
        assert_eq!(job_after.checked_count(), 1);

        let entries_after = repo.find_entries_by_job_id(job_id).await.unwrap();
        assert_eq!(entries_after.len(), 1);
        assert!(entries_after[0].result().is_some());
    }
}

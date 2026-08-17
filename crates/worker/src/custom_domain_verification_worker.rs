use std::{error::Error, sync::Arc, time::Duration as StdDuration};

use application::ports::{CustomDomainVerifier, StatusPageRepository};
use chrono::{Duration as ChronoDuration, Utc};
use domain::entities::{CustomDomainVerification, StatusPage};
use tracing::{debug, error, info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// A `Pending` custom-domain request older than this is abandoned rather
/// than checked forever. The domain entity's `request_custom_domain` doc
/// comment calls this out explicitly ("`requested_at` lets the
/// worker/cleanup job expire stale, never-verified requests") — this is
/// that job. 7 days comfortably covers realistic DNS propagation delays
/// and an owner who's slow to update their DNS records, while not leaving
/// abandoned verification attempts (and the token that goes with them)
/// sitting around forever.
const STALE_PENDING_DOMAIN_MAX_AGE: ChronoDuration = ChronoDuration::days(7);

/// Runs a periodic sweep that checks every status page with a `Pending`
/// custom domain against live DNS, via `CustomDomainVerifier`.
///
/// SECURITY: this worker is the ONLY caller in the entire codebase
/// permitted to invoke `StatusPage::mark_custom_domain_verified` — and
/// only ever immediately after `CustomDomainVerifier::verify` has
/// returned `Ok(true)` for that exact domain/token pair. See the SECURITY
/// notes on both of those for why: skipping the real DNS check would let
/// anyone claim ownership of any domain and hijack its status-page
/// traffic once host-header routing goes live.
#[derive(Clone)]
pub struct CustomDomainVerificationWorker {
    status_page_repository: Arc<dyn StatusPageRepository>,
    custom_domain_verifier: Arc<dyn CustomDomainVerifier>,
}

impl CustomDomainVerificationWorker {
    pub fn new(
        status_page_repository: Arc<dyn StatusPageRepository>,
        custom_domain_verifier: Arc<dyn CustomDomainVerifier>,
    ) -> Self {
        Self {
            status_page_repository,
            custom_domain_verifier,
        }
    }

    /// Runs one sweep. Returns Ok(()) even if individual pages fail to
    /// verify; errors are logged and the sweep continues to the next
    /// page.
    pub async fn run_once(&self) -> Result<()> {
        let pending_pages = self
            .status_page_repository
            .find_pending_custom_domains()
            .await?;

        debug!(
            count = pending_pages.len(),
            "custom domain verification sweep starting"
        );

        for mut status_page in pending_pages {
            if let Err(err) = self.process_status_page(&mut status_page).await {
                error!(
                    status_page_id = %status_page.id.0,
                    error = %err,
                    "failed to process custom domain verification for status page"
                );
                continue;
            }
        }

        Ok(())
    }

    async fn process_status_page(&self, status_page: &mut StatusPage) -> Result<()> {
        let Some((
            domain,
            CustomDomainVerification::Pending {
                token,
                requested_at,
            },
        )) = status_page.custom_domain.clone()
        else {
            // Already verified (or cleared) by the time we got to it —
            // nothing to do. Can happen if the owner updated it between
            // the list query and now.
            debug!(
                status_page_id = %status_page.id.0,
                "status page no longer has a pending custom domain; skipping"
            );
            return Ok(());
        };

        if Utc::now() - requested_at > STALE_PENDING_DOMAIN_MAX_AGE {
            warn!(
                status_page_id = %status_page.id.0,
                domain = domain.as_str(),
                requested_at = %requested_at,
                "custom domain verification request is stale; abandoning"
            );
            status_page.remove_custom_domain();
            self.status_page_repository.update(status_page).await?;
            return Ok(());
        }

        let verified = match self
            .custom_domain_verifier
            .verify(domain.as_str(), &token)
            .await
        {
            Ok(verified) => verified,
            Err(err) => {
                // A lookup failure (DNS timeout, resolver error, etc) is
                // transient — log and try again next sweep. Do NOT treat
                // this as "not verified forever"; the page stays Pending.
                warn!(
                    status_page_id = %status_page.id.0,
                    domain = domain.as_str(),
                    error = %err,
                    "DNS TXT record lookup failed during custom domain verification"
                );
                return Ok(());
            }
        };

        if !verified {
            debug!(
                status_page_id = %status_page.id.0,
                domain = domain.as_str(),
                "custom domain TXT record not published yet"
            );
            return Ok(());
        }

        // SECURITY: this is the one and only place in the codebase this
        // is called, and only immediately after a live DNS lookup
        // confirmed above returned `Ok(true)` for this exact
        // domain/token pair. See the SECURITY note on
        // `mark_custom_domain_verified` itself.
        status_page.mark_custom_domain_verified()?;
        self.status_page_repository.update(status_page).await?;

        info!(
            status_page_id = %status_page.id.0,
            domain = domain.as_str(),
            "custom domain verified"
        );

        Ok(())
    }

    /// Runs forever, sleeping 60 seconds between sweeps.
    ///
    /// Same cadence as `EscalationWorker` for consistency. DNS
    /// propagation is typically much slower than 60 seconds, so most
    /// sweeps for a given pending domain will simply find nothing new —
    /// that's expected and cheap (a single indexed query), not a
    /// problem to tune away.
    pub async fn run_forever(&self) -> Result<()> {
        let mut interval = tokio::time::interval(StdDuration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(err) = self.run_once().await {
                error!(error = %err, "custom domain verification sweep failed");
            }
        }
    }
}

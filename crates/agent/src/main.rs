mod checker;
mod claim_client;
mod config;

use anyhow::{Context, Result};
use reqwest::{Certificate, Client, Identity};
use std::{fs, sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::AgentConfig;

fn build_mtls_client(config: &AgentConfig) -> Result<Client> {
    let mut identity_buf = fs::read(&config.client_cert_path)
        .with_context(|| format!("failed to read client cert at {}", config.client_cert_path))?;
    let mut key_bytes = fs::read(&config.client_key_path)
        .with_context(|| format!("failed to read client key at {}", config.client_key_path))?;
    identity_buf.append(&mut key_bytes);

    let identity = Identity::from_pem(&identity_buf)
        .context("failed to build TLS identity from client cert/key")?;

    let ca_bytes = fs::read(&config.ca_cert_path)
        .with_context(|| format!("failed to read CA cert at {}", config.ca_cert_path))?;
    let ca_cert = Certificate::from_pem(&ca_bytes).context("failed to parse CA certificate")?;

    let client = Client::builder()
        .identity(identity)
        .add_root_certificate(ca_cert)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build mTLS HTTP client")?;

    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let agent_config = AgentConfig::from_env();
    let client = build_mtls_client(&agent_config)?;

    let geoip_reader = Arc::new(
        maxminddb::Reader::open_readfile(&agent_config.geoip_db_path).with_context(|| {
            format!(
                "failed to open GeoIP database at {}",
                agent_config.geoip_db_path
            )
        })?,
    );

    info!(
        agent_id = %agent_config.agent_id,
        api_base = %agent_config.api_base_url,
        geoip_db_path = %agent_config.geoip_db_path,
        max_concurrent_checks = agent_config.max_concurrent_checks,
        "agent started"
    );

    let semaphore = Arc::new(tokio::sync::Semaphore::new(
        agent_config.max_concurrent_checks,
    ));

    let poll_interval = Duration::from_secs(5);

    loop {
        match claim_client::claim_checks(
            &client,
            &agent_config.api_base_url,
            &agent_config.agent_id,
            &agent_config.agent_region,
        )
        .await
        {
            Ok(checks) => {
                if checks.is_empty() {
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }

                let mut tasks = JoinSet::new();

                for task in checks {
                    let client = client.clone();
                    let geoip_reader = Arc::clone(&geoip_reader);
                    let semaphore = Arc::clone(&semaphore);
                    let api_base = agent_config.api_base_url.clone();
                    let agent_id = agent_config.agent_id.clone();
                    let agent_region = agent_config.agent_region.clone();

                    tasks.spawn(async move {
                        let _permit = semaphore
                            .acquire_owned()
                            .await
                            .expect("agent semaphore must not be closed");

                        let result = checker::check_monitor(
                            &task.monitor,
                            &task.region,
                            geoip_reader.as_ref(),
                        )
                        .await;

                        if let Err(err) = claim_client::submit_result(
                            &client,
                            &api_base,
                            &agent_id,
                            &agent_region,
                            task.check_id,
                            task.monitor.id.0,
                            result,
                        )
                        .await
                        {
                            error!(
                                check_id = %task.check_id,
                                error = %err,
                                "failed to submit check result"
                            );
                        }
                    });
                }

                while let Some(join_result) = tasks.join_next().await {
                    if let Err(join_err) = join_result {
                        error!(error = %join_err, "agent check task panicked");
                    }
                }
            }
            Err(err) => {
                warn!(error = %err, "claim request failed; will retry");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

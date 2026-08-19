mod checker;
mod claim_client;

use anyhow::{Context, Result};
use reqwest::{Certificate, Client, Identity};
use std::{env, fs, time::Duration};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

fn build_mtls_client() -> Result<Client> {
    let client_cert_path = env::var("AGENT_CLIENT_CERT_PATH")
        .unwrap_or_else(|_| "certs/agents/agent-us-east-1/client-cert.pem".to_string());
    let client_key_path = env::var("AGENT_CLIENT_KEY_PATH")
        .unwrap_or_else(|_| "certs/agents/agent-us-east-1/client-key.pem".to_string());
    let ca_cert_path =
        env::var("AGENT_CA_CERT_PATH").unwrap_or_else(|_| "certs/ca/ca-cert.pem".to_string());

    let mut identity_buf = fs::read(&client_cert_path)
        .with_context(|| format!("failed to read client cert at {client_cert_path}"))?;
    let mut key_bytes = fs::read(&client_key_path)
        .with_context(|| format!("failed to read client key at {client_key_path}"))?;
    identity_buf.append(&mut key_bytes);

    let identity = Identity::from_pem(&identity_buf)
        .context("failed to build TLS identity from client cert/key")?;

    let ca_bytes = fs::read(&ca_cert_path)
        .with_context(|| format!("failed to read CA cert at {ca_cert_path}"))?;
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

    let api_base = env::var("AGENT_API_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080/internal/v1".to_string());
    let agent_id = env::var("AGENT_ID").unwrap_or_else(|_| "agent-default".to_string());
    let agent_region = env::var("AGENT_REGION").unwrap_or_else(|_| "us-east".to_string());

    let client = build_mtls_client()?;

    let geoip_db_path = env::var("GEOIP_DB_PATH")
        .unwrap_or_else(|_| "data/geoip/GeoLite2-Country.mmdb".to_string());
    let geoip_reader = maxminddb::Reader::open_readfile(&geoip_db_path)
        .with_context(|| format!("failed to open GeoIP database at {geoip_db_path}"))?;

    info!(
        agent_id = %agent_id,
        api_base = %api_base,
        geoip_db_path = %geoip_db_path,
        "agent started"
    );

    let poll_interval = Duration::from_secs(5);

    loop {
        match claim_client::claim_checks(&client, &api_base, &agent_id, &agent_region).await {
            Ok(checks) => {
                if checks.is_empty() {
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }

                for task in checks {
                    let result =
                        checker::check_monitor(&task.monitor, &task.region, &geoip_reader).await;

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
                }
            }
            Err(err) => {
                warn!(error = %err, "claim request failed; will retry");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

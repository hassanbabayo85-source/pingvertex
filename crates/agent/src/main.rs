mod checker;
mod claim_client;

use anyhow::Result;
use reqwest::Client;
use std::{env, time::Duration};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

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
    let agent_secret = env::var("AGENT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let agent_region = env::var("AGENT_REGION").unwrap_or_else(|_| "us-east".to_string());

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    info!(
        agent_id = %agent_id,
        api_base = %api_base,
        "agent started"
    );

    let poll_interval = Duration::from_secs(5);

    loop {
        match claim_client::claim_checks(
            &client,
            &api_base,
            &agent_id,
            &agent_secret,
            &agent_region,
        )
        .await
        {
            Ok(checks) => {
                if checks.is_empty() {
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }

                for task in checks {
                    let result = checker::check_monitor(&task.monitor, &task.region).await;

                    if let Err(err) = claim_client::submit_result(
                        &client,
                        &api_base,
                        &agent_id,
                        &agent_secret,
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

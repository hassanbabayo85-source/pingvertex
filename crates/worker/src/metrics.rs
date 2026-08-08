use std::{error::Error, net::SocketAddr, sync::OnceLock};

use axum::{Router, routing::get};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::info;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder for worker");

    let _ = PROMETHEUS_HANDLE.set(handle);
}

pub fn render_metrics() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map(|handle| handle.render())
        .unwrap_or_default()
}

pub async fn serve_metrics() -> Result<(), Box<dyn Error + Send + Sync>> {
    let app = Router::new().route("/metrics", get(|| async { render_metrics() }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("Worker metrics listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

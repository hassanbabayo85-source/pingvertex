use std::time::Instant;

use axum::{body::Body, http::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};

pub async fn record_http_metrics(req: Request<Body>, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let path = req.uri().path().to_owned();

    if path == "/metrics" {
        return next.run(req).await;
    }

    let start = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();

    counter!("http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.clone()
    )
    .increment(1);

    histogram!("http_request_duration_seconds",
        "method" => method,
        "path" => path,
        "status" => status
    )
    .record(elapsed);

    response
}

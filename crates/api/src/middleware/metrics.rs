use std::time::Instant;

use axum::{body::Body, extract::MatchedPath, http::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};

pub async fn record_http_metrics(req: Request<Body>, next: Next) -> Response {
    let method = req.method().as_str().to_owned();

    // Use the Axum route template (e.g. /api/v1/monitors/{id}) instead
    // of the raw URI path. Raw paths with UUIDs/hostnames create
    // unbounded Prometheus label cardinality; MatchedPath collapses
    // thousands of distinct URLs into a single stable label.
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, middleware, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn uses_matched_path_template_not_raw_uuid() {
        async fn handler() -> &'static str {
            "ok"
        }

        let app = Router::new()
            .route("/api/v1/monitors/{id}", get(handler))
            .layer(middleware::from_fn(record_http_metrics));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/monitors/123e4567-e89b-12d3-a456-426614174000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn matched_path_is_extracted() {
        async fn handler() -> &'static str {
            "ok"
        }

        let app = Router::new()
            .route("/api/v1/monitors/{id}", get(handler))
            .layer(middleware::from_fn(record_http_metrics));

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/monitors/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
    }
}

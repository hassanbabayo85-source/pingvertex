use axum::http::StatusCode;
use serde_json::json;
use tower::util::ServiceExt;

mod test_app;

#[tokio::test]
async fn pricing_quote_returns_full_breakdown() {
    let app = test_app::build().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pricing/quote")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "monitors": 500,
                        "interval_seconds": 30,
                        "retention_days": 90
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("compute").is_some());
    assert!(json.get("storage").is_some());
    assert!(json.get("monthly_price").is_some());
    assert_eq!(json["pricing_version"], 1);
}

#[tokio::test]
async fn pricing_quote_validation_error() {
    let app = test_app::build().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pricing/quote")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "monitors": 0,
                        "interval_seconds": 30,
                        "retention_days": 90
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

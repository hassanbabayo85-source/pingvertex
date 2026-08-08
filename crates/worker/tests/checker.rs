use std::net::SocketAddr;

use domain::shared::ids::{MonitorId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
};
use reqwest::Client;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use uuid::Uuid;

use worker::check_monitor;

async fn spawn_http_server(status: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0_u8; 1024];
            let _ = socket.read(&mut buf).await;

            let reason = match status {
                200 => "OK",
                500 => "Internal Server Error",
                503 => "Service Unavailable",
                _ => "OK",
            };

            let response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
            );

            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    format!("http://{addr}")
}

fn build_monitor(url: String) -> domain::Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), UserId(Uuid::new_v4()))
        .name(MonitorName::new("Test Monitor").unwrap())
        .url(Url::new(url).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build()
}

#[tokio::test]
async fn returns_failure_when_monitor_is_disabled() {
    let client = Client::new();
    let mut monitor = build_monitor("https://example.com".into());
    monitor.enabled = false;

    let result = check_monitor(&client, &monitor).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.status_code, None);
    assert_eq!(result.error.as_deref(), Some("monitor is disabled"));
}

#[tokio::test]
async fn returns_success_for_healthy_monitor() {
    let client = Client::new();
    let url = spawn_http_server(200).await;
    let monitor = build_monitor(url);

    let result = check_monitor(&client, &monitor).await.unwrap();

    assert!(result.success);
    assert_eq!(result.status_code, Some(200));
    assert!(result.error.is_none());
}

#[tokio::test]
async fn returns_failure_for_unexpected_status() {
    let client = Client::new();
    let url = spawn_http_server(503).await;
    let monitor = build_monitor(url);

    let result = check_monitor(&client, &monitor).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.status_code, None);
    assert!(
        result
            .error
            .as_deref()
            .unwrap()
            .contains("expected 200, got 503")
    );
}

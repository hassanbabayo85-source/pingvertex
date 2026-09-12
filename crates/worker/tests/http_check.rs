use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use domain::shared::ids::{MonitorId, TenantId, UserId};
use domain::{
    CheckErrorKind, CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName,
    Timeout, Url,
};
use reqwest::Client;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use uuid::Uuid;

use worker::{check_http_with_validator, init_rustls_crypto_provider};

async fn spawn_http_server(status: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

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

async fn spawn_hanging_http_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((_socket, _)) = listener.accept().await {
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    format!("http://{addr}")
}

fn build_monitor(url: String) -> domain::Monitor {
    MonitorBuilder::new(
        MonitorId(Uuid::new_v4()),
        UserId(Uuid::new_v4()),
        TenantId::new(),
    )
    .name(MonitorName::new("HTTP Check Test Monitor").unwrap())
    .url(Url::new(url).unwrap())
    .method(HttpMethod::Get)
    .expected_status(ExpectedStatus::new(200).unwrap())
    .interval(CheckInterval::new(60).unwrap())
    .timeout(Timeout::new(2).unwrap())
    .build()
}

#[tokio::test]
async fn success_returns_200_and_no_error() {
    let client = Client::new();
    let url = spawn_http_server(200).await;
    let monitor = build_monitor(url);

    let outcome = check_http_with_validator(&client, &monitor, |_| async { Ok(()) })
        .await
        .unwrap();

    assert!(outcome.result.success);
    assert_eq!(outcome.result.status_code, Some(200));
    assert!(outcome.result.error.is_none());
    assert_eq!(outcome.egress_bytes, 2);
}

#[tokio::test]
async fn wrong_status_fails_with_http_error_and_captures_status() {
    let client = Client::new();
    let url = spawn_http_server(503).await;
    let monitor = build_monitor(url);

    let outcome = check_http_with_validator(&client, &monitor, |_| async { Ok(()) })
        .await
        .unwrap();

    assert!(!outcome.result.success);
    assert_eq!(outcome.result.status_code, Some(503));
    assert_eq!(outcome.error_kind, CheckErrorKind::HttpError);
}

#[tokio::test]
async fn timeout_fails_with_timeout_error() {
    let client = Client::new();
    let url = spawn_hanging_http_server().await;
    let monitor = build_monitor(url);

    let outcome = check_http_with_validator(&client, &monitor, |_| async { Ok(()) })
        .await
        .unwrap();

    assert!(!outcome.result.success);
    assert_eq!(outcome.error_kind, CheckErrorKind::Timeout);
}

async fn spawn_self_signed_tls_server() -> SocketAddr {
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(30);

    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivatePkcs8KeyDer::from(key_pair.serialize_der());

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert_der],
            rustls::pki_types::PrivateKeyDer::Pkcs8(key_der),
        )
        .expect("valid self-signed server config");

    let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));

    let std_listener =
        std::net::TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind TLS server");
    std_listener.set_nonblocking(true).unwrap();
    let listener = TcpListener::from_std(std_listener).expect("tokio TLS listener");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = acceptor.accept(stream).await;
        }
    });

    addr
}

#[tokio::test]
async fn tls_failure_fails_with_tls_error() {
    init_rustls_crypto_provider();

    let addr = spawn_self_signed_tls_server().await;
    let monitor = build_monitor(format!("https://localhost:{}", addr.port()));
    let client = Client::new();

    let outcome = check_http_with_validator(&client, &monitor, |_| async { Ok(()) })
        .await
        .unwrap();

    assert!(!outcome.result.success);
    assert_eq!(outcome.error_kind, CheckErrorKind::TlsError);
}

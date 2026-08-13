use std::net::SocketAddr;

use domain::entities::Assertion;
use domain::enums::{AssertionOperator, ResponsePart};
use domain::shared::ids::{AssertionId, MonitorId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
};
use reqwest::Client;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use uuid::Uuid;

use worker::{check_monitor, check_monitor_with_validator};

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

    let result = check_monitor_with_validator(&client, &monitor, |_url: String| async { Ok(()) })
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.status_code, Some(200));
    assert!(result.error.is_none());
}

#[tokio::test]
async fn returns_failure_for_unexpected_status() {
    let client = Client::new();
    let url = spawn_http_server(503).await;
    let monitor = build_monitor(url);

    let result = check_monitor_with_validator(&client, &monitor, |_url: String| async { Ok(()) })
        .await
        .unwrap();

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

// ── Sabbin gwaje-gwaje na assertion ──

#[tokio::test]
async fn returns_success_when_body_assertion_passes() {
    let client = Client::new();
    let url = spawn_http_server(200).await;

    let mut monitor = build_monitor(url);

    monitor.assertions.push(Assertion {
        id: AssertionId(Uuid::new_v4()),
        response_part: ResponsePart::Body,
        operator: AssertionOperator::Contains,
        target: None,
        expected_value: Some("ok".into()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });

    let result = check_monitor_with_validator(&client, &monitor, |_url: String| async { Ok(()) })
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.status_code, Some(200));
    assert!(result.error.is_none());
}

#[tokio::test]
async fn returns_failure_when_body_assertion_fails() {
    let client = Client::new();
    let url = spawn_http_server(200).await;

    let mut monitor = build_monitor(url);

    monitor.assertions.push(Assertion {
        id: AssertionId(Uuid::new_v4()),
        response_part: ResponsePart::Body,
        operator: AssertionOperator::Contains,
        target: None,
        expected_value: Some("NOT_PRESENT".into()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });

    let result = check_monitor_with_validator(&client, &monitor, |_url: String| async { Ok(()) })
        .await
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.status_code, Some(200));
    assert!(result.error.is_some());
    let err_msg = result.error.unwrap();
    assert!(
        err_msg.contains("assertions sun gaza"),
        "Kuskuren ya ƙunshi 'assertions sun gaza': {err_msg}"
    );
}

// ── Sabbin gwaje-gwaje na TLS certificate checking (SEC-independent — sabon
// fasali ne, ba wani security fix ba) ──
//
// NOTE akan tsarin gwajin da ke ƙasa: mun gwada `maybe_fetch_cert_info`
// KAI TSAYE (ba ta hanyar `check_monitor` gaba ɗaya ba), domin
// `check_monitor` yana amfani da ainihin
// `crate::ssrf::resolve_and_validate_destination`, wanda — daidai kamar
// yadda aka tsara shi domin toshe SSRF — yana KIN duk wani adireshin
// loopback/127.0.0.1 kai tsaye. Wannan yana nufin ba za a taɓa iya kiran
// `check_monitor` (ba `check_monitor_with_validator` ba) akan wani local
// test server ba, koda HTTP ko HTTPS ne. Wannan salon gwajin (kiran
// `maybe_fetch_cert_info` kai tsaye da wani socket_addr da aka riga aka
// warware) daidai yake da yadda `tls_cert.rs`'s own tests da
// `check_monitor_with_validator` duka biyu ke gujewa wannan matsala.
mod cert_checking {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
    use std::sync::Arc;

    use domain::shared::ids::{MonitorId, UserId};
    use domain::{ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url};
    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;
    use uuid::Uuid;

    use worker::maybe_fetch_cert_info;

    fn build_https_monitor(url: String) -> domain::Monitor {
        let mut monitor = MonitorBuilder::new(MonitorId(Uuid::new_v4()), UserId(Uuid::new_v4()))
            .name(MonitorName::new("Test HTTPS Monitor").unwrap())
            .url(Url::new(url).unwrap())
            .method(HttpMethod::Get)
            .expected_status(ExpectedStatus::new(200).unwrap())
            .interval(domain::CheckInterval::new(60).unwrap())
            .timeout(Timeout::new(5).unwrap())
            .build();
        monitor.ssl_check_enabled = true;
        monitor
    }

    /// Spins up a local TLS "server" presenting a self-signed cert. It only
    /// needs to complete the TLS handshake — `maybe_fetch_cert_info` /
    /// `fetch_certificate_info` don't send or read any application data.
    ///
    /// This intentionally duplicates (rather than imports) the equivalent
    /// helper in `crates/worker/src/tls_cert.rs`'s own `#[cfg(test)]`
    /// module: that module is private to `tls_cert.rs` and is not part of
    /// the `worker` crate's public API, so it isn't reachable from this
    /// external integration-test crate.
    fn install_rustls_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    async fn spawn_self_signed_tls_server() -> SocketAddr {
        install_rustls_provider();
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

        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let std_listener =
            StdTcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind test server");
        std_listener.set_nonblocking(true).unwrap();
        let listener = TcpListener::from_std(std_listener).expect("tokio listener");
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let _ = acceptor.accept(stream).await;
            }
        });

        addr
    }

    #[tokio::test]
    async fn cert_info_stays_none_when_ssl_check_disabled() {
        let mut monitor = build_https_monitor("https://example.invalid".to_string());
        monitor.ssl_check_enabled = false;

        // Deliberately an address nothing is listening on — if
        // `maybe_fetch_cert_info` incorrectly tried to connect despite
        // `ssl_check_enabled == false`, this would hang/error instead of
        // returning immediately.
        let unused_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let (expires_at, days_remaining) =
            maybe_fetch_cert_info(&monitor, "example.invalid", unused_addr).await;

        assert_eq!(expires_at, None);
        assert_eq!(days_remaining, None);
    }

    #[tokio::test]
    async fn cert_info_stays_none_for_non_https_monitor_even_if_ssl_check_enabled() {
        // ssl_check_enabled defaults to true via build_https_monitor, but
        // the URL is http://, which must also short-circuit.
        let monitor = build_https_monitor("http://example.invalid".to_string());
        let unused_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let (expires_at, days_remaining) =
            maybe_fetch_cert_info(&monitor, "example.invalid", unused_addr).await;

        assert_eq!(expires_at, None);
        assert_eq!(days_remaining, None);
    }

    #[tokio::test]
    async fn cert_fetch_failure_does_not_panic_or_propagate_when_cert_is_untrusted() {
        // `maybe_fetch_cert_info` calls the PRODUCTION `tls_cert::fetch_certificate_info`,
        // which verifies against the real platform root store and never
        // disables verification (see tls_cert.rs module docs). A
        // self-signed cert is therefore correctly rejected during the TLS
        // handshake here — this test asserts that rejection is handled
        // gracefully ((None, None), no panic, no error bubbling up to fail
        // the overall monitor check), NOT that cert info is populated.
        //
        // Getting a genuine `Some(_)` positive-path test (i.e. one where
        // the fetch actually succeeds) would require trusting the test
        // server's self-signed cert, which needs a `ClientConfig` built
        // from a custom root store. That capability already exists in
        // `tls_cert.rs` as the private `fetch_certificate_info_with_config`
        // helper used by ITS OWN unit tests — but per this task's scope,
        // `tls_cert.rs` is read-only, and that helper is not part of the
        // `worker` crate's public API, so it cannot be reached from this
        // external integration-test crate without modifying tls_cert.rs.
        let addr = spawn_self_signed_tls_server().await;
        let monitor = build_https_monitor(format!("https://localhost:{}", addr.port()));

        let (expires_at, days_remaining) = maybe_fetch_cert_info(&monitor, "localhost", addr).await;

        assert_eq!(
            expires_at, None,
            "an untrusted self-signed cert must not produce cert info"
        );
        assert_eq!(days_remaining, None);
    }
}

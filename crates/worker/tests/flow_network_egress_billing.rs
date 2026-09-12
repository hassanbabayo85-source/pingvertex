//! GAP 1 acceptance tests for the API flow path.
//!
//! Proves that `execute_flow_with_resolver` reports real per-step
//! `egress_bytes` (summed by `FlowCheckExecutor::execute` into the
//! `CheckExecution` that the scheduler sees), and that
//! `bill_network_egress` charges those bytes with `MonitorKind::Flow`
//! using the existing 1 MiB round-up + PAYG pricing machinery.
//!
//! Also proves the two boundary conditions from the GAP's acceptance
//! criteria: zero-byte flow charges nothing, and insufficient wallet
//! balance is a logged no-op (never panics, never charges).

use std::net::SocketAddr;
use std::sync::Arc;

use application::ports::{PAYGPricingRepository, WalletRepository};
use application::MeterUsageUseCase;
use chrono::{Duration, Utc};
use domain::entities::{ApiFlow, ApiFlowStep};
use domain::shared::ids::{MonitorId, TenantId};
use domain::{HttpMethod, MonitorKind, WalletTransactionType};
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
use infra::{InMemoryUsageEventRepository, InMemoryWalletRepository};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;
use worker::bill_network_egress;
use worker::flow_executor::{DestinationResolverFn, execute_flow_with_resolver};

// -------- Fixtures ---------------------------------------------------

fn step(order: u16, name: &str, url_template: &str) -> ApiFlowStep {
    ApiFlowStep::new(
        order,
        domain::value_objects::FlowStepName::new(name).unwrap(),
        HttpMethod::Get,
        url_template.to_string(),
        vec![],
        None,
        vec![],
        vec![],
    )
    .unwrap()
}

fn flow(steps: Vec<ApiFlowStep>) -> ApiFlow {
    ApiFlow::new(
        domain::shared::ids::ApiFlowId::new(),
        MonitorId::new(),
        TenantId::new(),
        steps,
    )
    .unwrap()
}

/// Minimal one-shot-per-connection HTTP/1.1 mock server. Same shape as
/// the one in `flow_execution.rs` so the test seam matches.
async fn spawn_mock_server(bodies: Vec<Vec<u8>>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        for body in bodies {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            }
        }
    });

    addr
}

/// Pins every URL to the mock server, bypassing real SSRF/DNS. Only
/// used for orchestration assertions (egress byte accounting), not SSRF.
fn mock_resolver(mock_addr: SocketAddr) -> Box<DestinationResolverFn> {
    Box::new(move |url: &str| {
        let url_owned = url.to_string();
        Box::pin(async move {
            let parsed = url::Url::parse(&url_owned).map_err(|e| e.to_string())?;
            let host = parsed.host_str().ok_or("no host")?.to_string();
            Ok(shared::ssrf::ResolvedDestination {
                url: parsed,
                host,
                socket_addr: mock_addr,
            })
        })
    })
}

async fn funded_billing_setup(balance_units: i64) -> (Arc<InMemoryWalletRepository>, MeterUsageUseCase, Uuid) {
    let tenant = Uuid::new_v4();
    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant, "USD").await.unwrap();
    if balance_units > 0 {
        wallet
            .credit(
                tenant,
                balance_units,
                WalletTransactionType::TopUp,
                "egress-flow-topup",
                None,
            )
            .await
            .unwrap();
    }
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let now = Utc::now() - Duration::minutes(1);
    PAYGPricingRepository::create_version(&*pricing, "network_egress", "MB", 100, "USD", now)
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = MeterUsageUseCase::new(wallet.clone(), pricing, usage);
    (wallet, meter, tenant)
}

async fn balance(wallet: &Arc<InMemoryWalletRepository>, tenant: Uuid) -> i64 {
    wallet.find(tenant).await.unwrap().unwrap().balance_units
}

// -------- Tests ------------------------------------------------------

/// Two steps × 500 body bytes each → 1000 bytes total.
/// `network_egress_units(1000) == 1` (rounds up to a single 1 MiB unit).
#[tokio::test]
async fn flow_executor_reports_per_step_egress_bytes() {
    let mock_addr = spawn_mock_server(vec![vec![b'a'; 500], vec![b'b'; 500]]).await;

    let f = flow(vec![
        step(
            1,
            "First",
            &format!("http://mockhost.test:{}/a", mock_addr.port()),
        ),
        step(
            2,
            "Second",
            &format!("http://mockhost.test:{}/b", mock_addr.port()),
        ),
    ]);

    let client = reqwest::Client::new();
    let resolver = mock_resolver(mock_addr);
    let result = execute_flow_with_resolver(&f, &client, &resolver).await;

    assert!(result.passed, "both steps should pass; outcomes: {:?}", result.outcomes);
    assert_eq!(result.outcomes.len(), 2);
    assert_eq!(result.outcomes[0].egress_bytes, 500);
    assert_eq!(result.outcomes[1].egress_bytes, 500);

    let total: u64 = result.outcomes.iter().map(|o| o.egress_bytes).sum();
    assert_eq!(total, 1000);
    assert_eq!(worker::durable_scheduler::network_egress_units(total), 1);
}

/// Same flow shape, but now drive the *billing* side: the summed bytes
/// are handed to `bill_network_egress` with `MonitorKind::Flow` and the
/// wallet is debited exactly 1 MiB unit × 100 micro-USD.
#[tokio::test]
async fn flow_egress_charge_matches_measured_bytes() {
    let mock_addr = spawn_mock_server(vec![vec![b'a'; 500], vec![b'b'; 500]]).await;
    let f = flow(vec![
        step(
            1,
            "First",
            &format!("http://mockhost.test:{}/a", mock_addr.port()),
        ),
        step(
            2,
            "Second",
            &format!("http://mockhost.test:{}/b", mock_addr.port()),
        ),
    ]);
    let client = reqwest::Client::new();
    let resolver = mock_resolver(mock_addr);
    let result = execute_flow_with_resolver(&f, &client, &resolver).await;
    let egress_bytes: u64 = result.outcomes.iter().map(|o| o.egress_bytes).sum();

    let (wallet, meter, tenant) = funded_billing_setup(10_000).await;
    let monitor_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    bill_network_egress(
        &meter,
        tenant,
        monitor_id,
        execution_id,
        MonitorKind::Flow,
        None,
        None,
        egress_bytes,
        format!("flow-egress:{monitor_id}:{execution_id}"),
    )
    .await;

    assert_eq!(
        balance(&wallet, tenant).await,
        10_000 - 100,
        "1 MiB round-up × 100 micro-USD = 100 micro-USD"
    );
}

/// A flow whose single step returns an empty body has zero egress.
/// `bill_network_egress` must be a no-op: the wallet stays untouched
/// and no usage event is written.
#[tokio::test]
async fn zero_byte_flow_charges_nothing() {
    let mock_addr = spawn_mock_server(vec![vec![]]).await;
    let f = flow(vec![step(
        1,
        "Empty",
        &format!("http://mockhost.test:{}/empty", mock_addr.port()),
    )]);
    let client = reqwest::Client::new();
    let resolver = mock_resolver(mock_addr);
    let result = execute_flow_with_resolver(&f, &client, &resolver).await;
    let egress_bytes: u64 = result.outcomes.iter().map(|o| o.egress_bytes).sum();
    assert_eq!(egress_bytes, 0);

    let (wallet, meter, tenant) = funded_billing_setup(10_000).await;
    let monitor_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    bill_network_egress(
        &meter,
        tenant,
        monitor_id,
        execution_id,
        MonitorKind::Flow,
        None,
        None,
        0,
        format!("flow-egress-zero:{monitor_id}:{execution_id}"),
    )
    .await;

    assert_eq!(balance(&wallet, tenant).await, 10_000);
}

/// Wallet with only 50 micro-USD, egress of exactly 1 MiB (= 100
/// micro-USD to charge). `bill_network_egress` must not panic and must
/// not debit the wallet; it just logs a warning.
#[tokio::test]
async fn insufficient_balance_is_a_logged_no_op() {
    let (wallet, meter, tenant) = funded_billing_setup(50).await;
    let monitor_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    bill_network_egress(
        &meter,
        tenant,
        monitor_id,
        execution_id,
        MonitorKind::Flow,
        None,
        None,
        1024 * 1024,
        format!("flow-egress-insufficient:{monitor_id}:{execution_id}"),
    )
    .await;

    assert_eq!(
        balance(&wallet, tenant).await,
        50,
        "insufficient balance must leave the wallet untouched"
    );
}

pub mod assertion_evaluator;
pub mod checker;
pub mod custom_domain_verification_worker;
pub mod diagnostics;
pub mod dns_checker;
pub mod escalation_worker;
pub mod flow_executor;
pub mod incident_manager;
pub mod placeholder_resolver;
pub mod scheduler;
pub mod tcp_checker;
pub mod tls_cert;

pub use checker::{check_monitor, check_monitor_with_validator, maybe_fetch_cert_info};
pub use custom_domain_verification_worker::CustomDomainVerificationWorker;
pub use escalation_worker::EscalationWorker;
pub use incident_manager::IncidentManager;
pub use scheduler::run;
pub use tls_cert::{CertInfo, fetch_certificate_info};

pub mod metrics;

pub mod ssrf;
pub fn init_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub mod proxy_check_worker;
pub mod proxy_checker;
pub use proxy_check_worker::ProxyCheckWorker;
pub use proxy_checker::run_proxy_check_job;

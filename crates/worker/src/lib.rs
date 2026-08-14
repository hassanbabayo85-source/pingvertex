pub mod assertion_evaluator; // sabon module
pub mod checker;
pub mod flow_executor;
pub mod incident_manager;
pub mod placeholder_resolver;
pub mod scheduler;
pub mod tls_cert;

pub use checker::{check_monitor, check_monitor_with_validator, maybe_fetch_cert_info};
pub use incident_manager::IncidentManager;
pub use scheduler::run;
pub use tls_cert::{CertInfo, fetch_certificate_info};

pub mod metrics;

pub mod ssrf;
pub fn init_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

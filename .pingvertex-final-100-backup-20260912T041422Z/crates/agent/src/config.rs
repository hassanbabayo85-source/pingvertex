use std::env;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub client_cert_path: String,
    pub client_key_path: String,
    pub ca_cert_path: String,
    pub api_base_url: String,
    pub agent_id: String,
    pub agent_region: String,
    pub geoip_db_path: String,
    pub max_concurrent_checks: usize,
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_or_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

impl AgentConfig {
    pub fn from_env() -> Self {
        Self {
            client_cert_path: env_or(
                "AGENT_CLIENT_CERT_PATH",
                "certs/agents/agent-us-east-1/client-cert.pem",
            ),
            client_key_path: env_or(
                "AGENT_CLIENT_KEY_PATH",
                "certs/agents/agent-us-east-1/client-key.pem",
            ),
            ca_cert_path: env_or("AGENT_CA_CERT_PATH", "certs/ca/ca-cert.pem"),
            api_base_url: env_or("AGENT_API_BASE_URL", "http://localhost:8080/internal/v1"),
            agent_id: env_or("AGENT_ID", "agent-default"),
            agent_region: env_or("AGENT_REGION", "us-east"),
            geoip_db_path: env_or("GEOIP_DB_PATH", "data/geoip/GeoLite2-Country.mmdb"),
            max_concurrent_checks: env_or_usize("AGENT_MAX_CONCURRENT_CHECKS", 4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_previous_hardcoded_fallbacks() {
        for key in [
            "AGENT_CLIENT_CERT_PATH",
            "AGENT_CLIENT_KEY_PATH",
            "AGENT_CA_CERT_PATH",
            "AGENT_API_BASE_URL",
            "AGENT_ID",
            "AGENT_REGION",
            "GEOIP_DB_PATH",
            "AGENT_MAX_CONCURRENT_CHECKS",
        ] {
            assert!(
                env::var(key).is_err(),
                "test assumes {key} is not set in the test environment"
            );
        }

        let config = AgentConfig::from_env();
        assert_eq!(
            config.client_cert_path,
            "certs/agents/agent-us-east-1/client-cert.pem"
        );
        assert_eq!(
            config.client_key_path,
            "certs/agents/agent-us-east-1/client-key.pem"
        );
        assert_eq!(config.ca_cert_path, "certs/ca/ca-cert.pem");
        assert_eq!(config.api_base_url, "http://localhost:8080/internal/v1");
        assert_eq!(config.agent_id, "agent-default");
        assert_eq!(config.agent_region, "us-east");
        assert_eq!(config.geoip_db_path, "data/geoip/GeoLite2-Country.mmdb");
        assert_eq!(config.max_concurrent_checks, 4);
    }
}

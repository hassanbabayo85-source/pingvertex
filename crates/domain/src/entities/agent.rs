use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lifecycle status of a registered agent.
///
/// `Revoked` is terminal: an operator who wants to re-enable an agent
/// must re-register it (with a fresh certificate). See
/// `migrations/0122_agent_registry.sql` — the DB trigger
/// `trg_agents_revoked_terminal` enforces this at the storage layer too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Active,
    Revoked,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Server-side identity record for an internal agent.
///
/// The region field is the ONLY source of truth for which region an
/// agent may act on. Client-supplied region claims (headers or payload
/// bodies) are checked against this value by the middleware; a mismatch
/// is a hard rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: String,
    /// The region this agent is registered to operate in. Immutable once
    /// registered — changing it requires deleting and re-registering.
    pub region: String,
    /// Optional capability tags (e.g. `["http_check", "tcp_check"]`).
    /// Reserved for future per-endpoint authorization; presently
    /// informational.
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    /// Optional SHA-256 fingerprint of the agent's certificate (hex).
    /// If present, a future revision can pin the exact certificate —
    /// currently informational because the mTLS layer already enforces
    /// the CN matches the registered identity.
    pub certificate_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Agent {
    pub fn new(agent_id: impl Into<String>, region: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            agent_id: agent_id.into(),
            region: region.into(),
            capabilities: Vec::new(),
            status: AgentStatus::Active,
            certificate_fingerprint: None,
            created_at: now,
            updated_at: now,
            revoked_at: None,
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_status(mut self, status: AgentStatus) -> Self {
        self.status = status;
        if matches!(status, AgentStatus::Revoked) {
            self.revoked_at = Some(Utc::now());
        }
        self
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, AgentStatus::Active)
    }
}

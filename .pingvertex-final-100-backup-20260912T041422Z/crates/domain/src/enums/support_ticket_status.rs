use serde::{Deserialize, Serialize};

/// Lifecycle status of a support ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTicketStatus {
    Open,
    InProgress,
    WaitingForCustomer,
    Resolved,
    Closed,
}

impl SupportTicketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::WaitingForCustomer => "waiting_for_customer",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }

    /// Returns whether a transition to `target` is valid from this status.
    pub fn can_transition_to(&self, target: SupportTicketStatus) -> bool {
        use SupportTicketStatus::*;
        matches!(
            (self, target),
            (Open, InProgress)
                | (Open, WaitingForCustomer)
                | (Open, Resolved)
                | (Open, Closed)
                | (InProgress, WaitingForCustomer)
                | (InProgress, Resolved)
                | (InProgress, Closed)
                | (WaitingForCustomer, InProgress)
                | (WaitingForCustomer, Resolved)
                | (WaitingForCustomer, Closed)
                | (Resolved, Closed)
                | (Resolved, Open) // reopening a resolved ticket is allowed
        )
    }
}

impl std::fmt::Display for SupportTicketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SupportTicketStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "in_progress" => Ok(Self::InProgress),
            "waiting_for_customer" => Ok(Self::WaitingForCustomer),
            "resolved" => Ok(Self::Resolved),
            "closed" => Ok(Self::Closed),
            other => Err(format!("invalid support ticket status: {other}")),
        }
    }
}

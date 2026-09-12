use serde::{Deserialize, Serialize};

/// Categories for support tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportCategory {
    Account,
    Billing,
    Monitor,
    Notifications,
    Api,
    Security,
    TechnicalIssue,
    Other,
}

impl SupportCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Billing => "billing",
            Self::Monitor => "monitor",
            Self::Notifications => "notifications",
            Self::Api => "api",
            Self::Security => "security",
            Self::TechnicalIssue => "technical_issue",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for SupportCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SupportCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "account" => Ok(Self::Account),
            "billing" => Ok(Self::Billing),
            "monitor" => Ok(Self::Monitor),
            "notifications" => Ok(Self::Notifications),
            "api" => Ok(Self::Api),
            "security" => Ok(Self::Security),
            "technical_issue" => Ok(Self::TechnicalIssue),
            "other" => Ok(Self::Other),
            other => Err(format!("invalid support category: {other}")),
        }
    }
}

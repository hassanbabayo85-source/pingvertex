use serde::{Deserialize, Serialize};

/// PART 3 — three-way outcome classification for a proxy check.
///
/// The legacy `ProxyResult.reachable` field conflates two distinct
/// questions into one boolean:
///
///   1. Is the proxy itself alive (TCP connectable, protocol handshake
///      succeeded, no auth rejection)?
///   2. Is the verification target (example.com) reachable THROUGH the
///      proxy?
///
/// Those are not the same question. A user picking proxies to use does
/// not want a good proxy discarded just because a specific verification
/// target had a transient problem, and does not want a dead proxy kept
/// just because the target happens to be up when the proxy is not.
///
/// This enum separates them:
///
/// - `Working`                    — both proxy AND target worked.
/// - `WorkingTargetUnreachable`   — proxy worked; ONLY the target failed.
///   The proxy is ALIVE. A caller that filters "working proxies" MUST
///   include this case. A caller showing "fully verified" MUST exclude it.
/// - `ProxyFailed`                — the proxy itself failed. This is the
///   only case that may be treated as a dead/broken proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyCheckOutcome {
    Working,
    WorkingTargetUnreachable,
    ProxyFailed,
}

impl ProxyCheckOutcome {
    /// `true` for any outcome where the proxy itself is alive — i.e.
    /// `Working` OR `WorkingTargetUnreachable`.
    pub fn proxy_is_alive(self) -> bool {
        matches!(self, Self::Working | Self::WorkingTargetUnreachable)
    }

    /// `true` only when the proxy AND the target both succeeded.
    pub fn fully_working(self) -> bool {
        matches!(self, Self::Working)
    }

    /// `true` only when the failure was on the target side, not the
    /// proxy side.
    pub fn is_target_failure(self) -> bool {
        matches!(self, Self::WorkingTargetUnreachable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_is_alive_true_for_working_and_target_failure() {
        assert!(ProxyCheckOutcome::Working.proxy_is_alive());
        assert!(ProxyCheckOutcome::WorkingTargetUnreachable.proxy_is_alive());
        assert!(!ProxyCheckOutcome::ProxyFailed.proxy_is_alive());
    }

    #[test]
    fn fully_working_only_for_working() {
        assert!(ProxyCheckOutcome::Working.fully_working());
        assert!(!ProxyCheckOutcome::WorkingTargetUnreachable.fully_working());
        assert!(!ProxyCheckOutcome::ProxyFailed.fully_working());
    }

    #[test]
    fn is_target_failure_only_for_target_failure() {
        assert!(!ProxyCheckOutcome::Working.is_target_failure());
        assert!(ProxyCheckOutcome::WorkingTargetUnreachable.is_target_failure());
        assert!(!ProxyCheckOutcome::ProxyFailed.is_target_failure());
    }

    #[test]
    fn serde_roundtrip_uses_snake_case() {
        let json = serde_json::to_string(&ProxyCheckOutcome::WorkingTargetUnreachable).unwrap();
        assert_eq!(json, "\"working_target_unreachable\"");
        let back: ProxyCheckOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ProxyCheckOutcome::WorkingTargetUnreachable);
    }
}

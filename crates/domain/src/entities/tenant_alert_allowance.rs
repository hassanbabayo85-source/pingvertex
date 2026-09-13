use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::TenantId;

/// A tenant's fair-use alert allowance for one monthly billing cycle.
///
/// The allowance is proportional to the number of *enabled* monitors the
/// tenant has at the start of the cycle. It is consumed before the PAYG
/// wallet: the first `sms_limit` SMS alerts in a cycle are free, and only
/// once the allowance is exhausted does SMS delivery fall back to (a)
/// paid SMS credits in the wallet, or (b) the email channel.
///
/// Voice minutes and phone numbers are modeled for forward
/// compatibility. Today both limits are always 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantAlertAllowance {
    pub tenant_id: TenantId,
    pub cycle_started_at: DateTime<Utc>,
    pub cycle_ends_at: DateTime<Utc>,
    pub sms_limit: i32,
    pub sms_used: i32,
    pub voice_limit: i32,
    pub voice_used: i32,
    pub phone_numbers_limit: i32,
    pub phone_numbers_used: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TenantAlertAllowance {
    /// Baseline SMS allowance per 1,000 enabled monitors.
    pub const SMS_PER_1000_MONITORS: u64 = 250;
    /// Baseline voice-minute allowance per 1,000 enabled monitors.
    /// Reserved for a future Voice integration.
    pub const VOICE_MINUTES_PER_1000_MONITORS: u64 = 0;
    /// Baseline phone-number allowance per 1,000 enabled monitors.
    /// Reserved for a future Voice integration.
    pub const PHONE_NUMBERS_PER_1000_MONITORS: u64 = 0;

    /// `ceil(enabled_monitors * 250 / 1000)`. A tenant with 1 monitor
    /// gets 1 SMS; 10 gets 3; 1,000 gets 250; 1,001 gets 251.
    pub fn compute_sms_limit(enabled_monitors: u64) -> i32 {
        if enabled_monitors == 0 {
            return 0;
        }
        let numerator = enabled_monitors.saturating_mul(Self::SMS_PER_1000_MONITORS);
        let ceiling = numerator.saturating_add(999) / 1000;
        ceiling.min(i32::MAX as u64) as i32
    }

    pub fn has_sms_remaining(&self) -> bool {
        self.sms_used < self.sms_limit
    }

    pub fn sms_remaining(&self) -> i32 {
        (self.sms_limit - self.sms_used).max(0)
    }
}

use domain::{Check, CheckStatus, ErrorType, RegionCode};
use domain::shared::ids::{CheckId, MonitorId};
use uuid::Uuid;

pub fn sample_check(monitor_id: MonitorId) -> Check {
    Check::new(
        CheckId(Uuid::new_v4()),
        monitor_id,
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        123,
        ErrorType::None,
        None,
    )
}

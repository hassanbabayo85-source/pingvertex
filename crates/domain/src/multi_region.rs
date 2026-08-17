use chrono::{DateTime, Duration, Utc};

use crate::entities::Check;
use crate::enums::CheckStatus;
use crate::value_objects::RegionCode;

const EVALUATION_WINDOW_SECS: i64 = 60;

pub fn evaluate_multi_region_status(
    configured_regions: &[RegionCode],
    recent_checks: &[Check],
    now: DateTime<Utc>,
) -> CheckStatus {
    if configured_regions.is_empty() {
        return CheckStatus::Up;
    }

    let cutoff = now - Duration::seconds(EVALUATION_WINDOW_SECS);

    let down_regions: usize = configured_regions
        .iter()
        .filter(|region| {
            recent_checks
                .iter()
                .filter(|check| check.checked_at >= cutoff)
                .any(|check| &check.region == *region && check.status == CheckStatus::Down)
        })
        .count();

    let total = configured_regions.len();
    let down_percent = (down_regions as f64 / total as f64) * 100.0;

    if down_percent > 50.0 {
        CheckStatus::Down
    } else {
        CheckStatus::Up
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::{CheckId, MonitorId};
    use crate::value_objects::RegionCode;

    fn us_east() -> RegionCode {
        RegionCode::new("us-east").unwrap()
    }

    fn eu_west() -> RegionCode {
        RegionCode::new("eu-west").unwrap()
    }

    fn make_check(region: RegionCode, status: CheckStatus, seconds_ago: i64) -> Check {
        let mut check = Check::new(
            CheckId::new(),
            MonitorId::new(),
            region,
            status,
            None,
            120,
            crate::enums::ErrorType::None,
            None,
        );
        check.checked_at = Utc::now() - Duration::seconds(seconds_ago);
        check
    }

    #[test]
    fn two_regions_one_down_does_not_trigger() {
        let regions = vec![us_east(), eu_west()];
        let checks = vec![
            make_check(us_east(), CheckStatus::Down, 10),
            make_check(eu_west(), CheckStatus::Up, 10),
        ];
        assert_eq!(
            evaluate_multi_region_status(&regions, &checks, Utc::now()),
            CheckStatus::Up
        );
    }

    #[test]
    fn two_regions_both_down_triggers() {
        let regions = vec![us_east(), eu_west()];
        let checks = vec![
            make_check(us_east(), CheckStatus::Down, 10),
            make_check(eu_west(), CheckStatus::Down, 10),
        ];
        assert_eq!(
            evaluate_multi_region_status(&regions, &checks, Utc::now()),
            CheckStatus::Down
        );
    }

    #[test]
    fn stale_down_checks_are_ignored() {
        let regions = vec![us_east(), eu_west()];
        let checks = vec![
            make_check(us_east(), CheckStatus::Down, 120),
            make_check(eu_west(), CheckStatus::Down, 120),
        ];
        assert_eq!(
            evaluate_multi_region_status(&regions, &checks, Utc::now()),
            CheckStatus::Up
        );
    }
}

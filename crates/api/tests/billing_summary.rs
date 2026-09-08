mod common;

use domain::{AddOnStatus, AddOnSubscription, AddOnType};
use rust_decimal::Decimal;

#[test]
fn billing_summary_fixture_addon_is_an_active_subscription_line_item() {
    let addon = AddOnSubscription::new(
        domain::AddOnSubscriptionId::new(),
        domain::shared::ids::TenantId::new(),
        AddOnType::EnterpriseSla,
        2,
        None,
        Decimal::new(9900, 2),
        "USD".into(),
        AddOnStatus::Active,
    )
    .unwrap();
    assert_eq!(addon.total_price(), Decimal::new(19800, 2));
}

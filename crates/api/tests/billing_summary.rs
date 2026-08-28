mod common;

use api::handlers::add_on_billing::billing_summary;
use axum::extract::{Extension, State};
use domain::{AddOnStatus, AddOnSubscription, AddOnType};
use rust_decimal::Decimal;

#[tokio::test]
async fn billing_summary_aggregates_base_and_active_addons_for_tenant() {
    let (state, user, _, _) = common::build_billing_summary_fixture().await;

    let axum::Json(summary) = billing_summary(State(state), Extension(user))
        .await
        .unwrap();

    assert_eq!(summary.add_ons.len(), 1);
    assert_eq!(summary.add_ons[0].addon_type, "enterprise");
    assert_eq!(summary.add_ons[0].status, "active");
    assert_eq!(summary.add_ons[0].total_price, "198.00");
    assert_eq!(summary.add_ons_total, "198.00");

    let base = summary.base_price.parse::<Decimal>().unwrap();
    let total = summary.total.parse::<Decimal>().unwrap();
    let add_ons_total = summary.add_ons_total.parse::<Decimal>().unwrap();
    assert_eq!(total, base + add_ons_total);
    assert_eq!(summary.currency, "USD");
}

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

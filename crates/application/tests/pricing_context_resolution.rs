use application::ports::{
    PAYGPricingRepository, PricingContext, PricingRuleContext, resolve_price,
};
use chrono::Utc;
use domain::{AccountType, MonitorKind, RegionCode};
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;

#[tokio::test]
async fn test_same_context_same_result() {
    let repo = InMemoryPAYGPricingRepository::new();
    repo.create_version("http_check.one_time", "check", 100, "USD", Utc::now())
        .await
        .unwrap();

    let ctx = PricingContext::default();

    let res1 = resolve_price(&repo, "http_check.one_time", &ctx)
        .await
        .unwrap()
        .unwrap();
    let res2 = resolve_price(&repo, "http_check.one_time", &ctx)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(res1.unit_price, 100);
    assert_eq!(res1.id, res2.id);
}

#[tokio::test]
async fn test_account_type_resolution() {
    let repo = InMemoryPAYGPricingRepository::new();

    let ctx_ind = PricingRuleContext {
        account_type: Some(AccountType::Individual),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        50,
        "USD",
        Utc::now(),
        &ctx_ind,
    )
    .await
    .unwrap();

    let ctx_comp = PricingRuleContext {
        account_type: Some(AccountType::Company),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        100,
        "USD",
        Utc::now(),
        &ctx_comp,
    )
    .await
    .unwrap();

    let res_ind = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            account_type: Some(AccountType::Individual),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    let res_comp = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            account_type: Some(AccountType::Company),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(res_ind.unit_price, 50);
    assert_eq!(res_comp.unit_price, 100);
}

#[tokio::test]
async fn test_region_resolution() {
    let repo = InMemoryPAYGPricingRepository::new();

    let ctx_a = PricingRuleContext {
        region: Some(RegionCode::new("us-east").unwrap()),
        ..Default::default()
    };
    repo.create_version_with_context("http_check.one_time", "unit", 10, "USD", Utc::now(), &ctx_a)
        .await
        .unwrap();

    let ctx_b = PricingRuleContext {
        region: Some(RegionCode::new("eu-west").unwrap()),
        ..Default::default()
    };
    repo.create_version_with_context("http_check.one_time", "unit", 20, "USD", Utc::now(), &ctx_b)
        .await
        .unwrap();

    let res_a = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            region: Some(RegionCode::new("us-east").unwrap()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    let res_b = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            region: Some(RegionCode::new("eu-west").unwrap()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(res_a.unit_price, 10);
    assert_eq!(res_b.unit_price, 20);
}

#[tokio::test]
async fn test_monitor_kind_resolution() {
    let repo = InMemoryPAYGPricingRepository::new();

    let ctx_http = PricingRuleContext {
        monitor_kind: Some(MonitorKind::Http),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        10,
        "USD",
        Utc::now(),
        &ctx_http,
    )
    .await
    .unwrap();

    let ctx_tcp = PricingRuleContext {
        monitor_kind: Some(MonitorKind::Tcp),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        15,
        "USD",
        Utc::now(),
        &ctx_tcp,
    )
    .await
    .unwrap();

    let res_http = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            monitor_kind: Some(MonitorKind::Http),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    let res_tcp = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            monitor_kind: Some(MonitorKind::Tcp),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(res_http.unit_price, 10);
    assert_eq!(res_tcp.unit_price, 15);
}

#[tokio::test]
async fn test_quantity_tier_resolution() {
    let repo = InMemoryPAYGPricingRepository::new();

    let ctx_tier1 = PricingRuleContext {
        max_quantity: Some(100),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        10,
        "USD",
        Utc::now(),
        &ctx_tier1,
    )
    .await
    .unwrap();

    let ctx_tier2 = PricingRuleContext {
        min_quantity: Some(101),
        ..Default::default()
    };
    repo.create_version_with_context(
        "http_check.one_time",
        "unit",
        5,
        "USD",
        Utc::now(),
        &ctx_tier2,
    )
    .await
    .unwrap();

    let res_low = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            quantity: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let res_bound = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            quantity: 100,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let res_high = resolve_price(
        &repo,
        "http_check.one_time",
        &PricingContext {
            quantity: 101,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(res_low.unit_price, 10);
    assert_eq!(res_bound.unit_price, 10);
    assert_eq!(res_high.unit_price, 5);
}

#[tokio::test]
async fn test_missing_price_fails_safely() {
    let repo = InMemoryPAYGPricingRepository::new();
    // Repository is empty
    let res = resolve_price(&repo, "test_op", &PricingContext::default()).await;
    // For invalid ops it errors out.
    assert!(res.is_err());

    // For valid op but no rules, it should safely return Ok(None)
    let res = resolve_price(&repo, "http_check.one_time", &PricingContext::default())
        .await
        .unwrap();
    assert!(res.is_none());
}

#[tokio::test]
async fn test_ambiguous_price_fails_safely() {
    let repo = InMemoryPAYGPricingRepository::new();

    let ctx1 = PricingRuleContext {
        account_type: Some(AccountType::Individual),
        ..Default::default()
    };
    repo.create_version_with_context("http_check.one_time", "unit", 10, "USD", Utc::now(), &ctx1)
        .await
        .unwrap();

    let ctx2 = PricingRuleContext {
        region: Some(RegionCode::new("us-east").unwrap()),
        ..Default::default()
    };
    repo.create_version_with_context("http_check.one_time", "unit", 20, "USD", Utc::now(), &ctx2)
        .await
        .unwrap();

    // Querying with BOTH individual and us-east-1 gives two rules of specificity = 1.
    // This is ambiguous.
    let ctx = PricingContext {
        account_type: Some(AccountType::Individual),
        region: Some(RegionCode::new("us-east").unwrap()),
        ..Default::default()
    };

    let res = resolve_price(&repo, "http_check.one_time", &ctx).await;
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().to_string(),
        "ambiguous pricing rules match the context with equal specificity"
    );
}

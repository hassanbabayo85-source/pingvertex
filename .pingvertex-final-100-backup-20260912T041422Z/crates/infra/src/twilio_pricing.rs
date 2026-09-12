use crate::twilio_config::TwilioConfig;
use application::ports::{SmsPricingResult, SmsProviderPricing};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::time::Duration;
#[derive(Deserialize)]
struct Country {
    price_unit: Option<String>,
    outbound_sms_prices: Option<Vec<Carrier>>,
}
#[derive(Deserialize)]
struct Carrier {
    prices: Option<Vec<Price>>,
}
#[derive(Deserialize)]
struct Price {
    current_price: Option<String>,
    base_price: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct TwilioPricingService;
impl TwilioPricingService {
    pub fn new() -> Self {
        Self
    }
}
#[async_trait]
impl SmsProviderPricing for TwilioPricingService {
    async fn outbound_sms_unit_price(&self, country: &str) -> SmsPricingResult<(Decimal, String)> {
        let code = country.trim().to_ascii_uppercase();
        if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err("iso_country must be a two-letter ISO 3166-1 code".into());
        }
        let cfg = TwilioConfig::try_from_env()?;
        let url = format!("https://pricing.twilio.com/v1/Messaging/Countries/{code}");
        let d = shared::ssrf::resolve_and_validate_destination(&url).await?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&d.host, d.socket_addr)
            .timeout(Duration::from_secs(5))
            .build()?;
        let r = client
            .get(d.url)
            .basic_auth(&cfg.account_sid, Some(&cfg.auth_token))
            .send()
            .await?;
        if !r.status().is_success() {
            return Err(format!(
                "twilio pricing endpoint responded with status {}",
                r.status()
            )
            .into());
        }
        let body: Country = r.json().await?;
        let mut vals = Vec::new();
        for carrier in body.outbound_sms_prices.unwrap_or_default() {
            for p in carrier.prices.unwrap_or_default() {
                if let Some(v) = p.current_price.or(p.base_price) {
                    vals.push(Decimal::from_str_exact(&v)?);
                }
            }
        }
        let price = vals.into_iter().min().ok_or_else(|| {
            std::io::Error::other("Twilio returned no outbound SMS price for country")
        })?;
        Ok((price, body.price_unit.unwrap_or_else(|| "USD".into())))
    }
}

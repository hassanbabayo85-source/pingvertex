use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
    RefundDetails, RefundRequest, WalletTopUpRequest,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct PaystackBillingProvider {
    client: Client,
    secret_key: String,
    base_url: String,
}
impl std::fmt::Debug for PaystackBillingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaystackBillingProvider")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}
impl PaystackBillingProvider {
    pub fn new(secret_key: String) -> PaymentProviderResult<Self> {
        if secret_key.trim().is_empty() {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("Paystack secret key is required"),
            ));
        }
        Ok(Self {
            client: Client::builder().timeout(Duration::from_secs(15)).build()?,
            secret_key,
            base_url: "https://api.paystack.co".into(),
        })
    }
    fn auth(&self, r: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        r.bearer_auth(&self.secret_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
    }
    fn wallet_to_minor(units: i64) -> PaymentProviderResult<i64> {
        if units <= 0 || units % 10_000 != 0 {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("wallet amount must be a whole USD cent"),
            ));
        }
        Ok(units / 10_000)
    }
    fn minor_to_wallet(amount: i64, currency: &str) -> PaymentProviderResult<i64> {
        if !currency.eq_ignore_ascii_case("USD") {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("PingVertex wallet currently supports USD only"),
            ));
        }
        amount.checked_mul(10_000).ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                "provider amount overflow",
            ))
        })
    }
    async fn initialize(
        &self,
        email: &str,
        amount: i64,
        currency: &str,
        reference: String,
    ) -> PaymentProviderResult<InitializeData> {
        let r = self
            .auth(
                self.client
                    .post(format!("{}/transaction/initialize", self.base_url)),
            )
            .json(&InitializeRequest {
                email,
                amount,
                currency,
                reference,
            })
            .send()
            .await?;
        let status = r.status();
        let body: PaystackResponse<InitializeData> = r.json().await?;
        if !status.is_success() || !body.status {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other(
                    body.message
                        .unwrap_or_else(|| format!("Paystack HTTP {}", status)),
                ),
            ));
        }
        body.data.ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                "Paystack returned no checkout data",
            ))
        })
    }
    async fn verify(&self, reference: &str) -> PaymentProviderResult<PaymentVerification> {
        let r = self
            .auth(self.client.get(format!(
                "{}/transaction/verify/{}",
                self.base_url, reference
            )))
            .send()
            .await?;
        let status = r.status();
        let body: PaystackResponse<TransactionData> = r.json().await?;
        if !status.is_success() || !body.status {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other(
                    body.message
                        .unwrap_or_else(|| format!("Paystack HTTP {}", status)),
                ),
            ));
        }
        let tx = body.data.ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                "Paystack returned no transaction",
            ))
        })?;
        let units = Self::minor_to_wallet(tx.amount, &tx.currency)?;
        let currency = tx.currency.to_ascii_uppercase();
        let pid = tx.id.to_string();
        Ok(match tx.status.as_str() {
            "success" => PaymentVerification::Paid {
                provider_payment_id: pid,
                provider_reference: tx.reference,
                amount_units: units,
                currency,
            },
            "pending" | "ongoing" | "processing" | "queued" => PaymentVerification::Pending {
                provider_payment_id: Some(pid),
                provider_reference: Some(tx.reference),
                amount_units: Some(units),
                currency: Some(currency),
            },
            _ => PaymentVerification::Failed {
                provider_payment_id: Some(pid),
                provider_reference: Some(tx.reference),
                amount_units: Some(units),
                currency: Some(currency),
            },
        })
    }
}
#[derive(Debug, Deserialize)]
struct PaystackResponse<T> {
    status: bool,
    message: Option<String>,
    data: Option<T>,
}
#[derive(Debug, Deserialize)]
struct InitializeData {
    authorization_url: String,
    reference: String,
}
#[derive(Debug, Deserialize)]
struct TransactionData {
    id: u64,
    status: String,
    reference: String,
    amount: i64,
    currency: String,
}
#[derive(Debug, Deserialize)]
struct RefundData {
    id: u64,
}
#[derive(Debug, Serialize)]
struct InitializeRequest<'a> {
    email: &'a str,
    amount: i64,
    currency: &'a str,
    reference: String,
}
#[derive(Debug, Serialize)]
struct ChargeAuthorizationRequest<'a> {
    email: &'a str,
    amount: i64,
    authorization_code: &'a str,
    reference: String,
    currency: &'a str,
}
#[derive(Debug, Serialize)]
struct RefundApiRequest<'a> {
    transaction: &'a str,
    amount: i64,
    currency: &'a str,
}
#[async_trait]
impl PaymentProvider for PaystackBillingProvider {
    fn provider_name(&self) -> &'static str {
        "paystack"
    }
    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        let email = request
            .customer_email
            .as_deref()
            .filter(|e| !e.trim().is_empty())
            .ok_or_else(|| {
                Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                    "customer email is required",
                ))
            })?;
        let d = self
            .initialize(
                email,
                request.amount_minor,
                &request.currency,
                format!("pv-sub-{}", uuid::Uuid::new_v4().simple()),
            )
            .await?;
        Ok(CheckoutDetails {
            provider_checkout_id: d.reference,
            checkout_url: d.authorization_url,
        })
    }
    async fn verify_payment(&self, reference: &str) -> PaymentProviderResult<PaymentVerification> {
        self.verify(reference).await
    }
    async fn create_wallet_top_up(
        &self,
        request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        if !request.currency.eq_ignore_ascii_case("USD") {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("wallet top-up currency must be USD"),
            ));
        }
        let email = request.customer_email.trim();
        if email.is_empty() {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("customer email is required"),
            ));
        }
        let d = self
            .initialize(
                email,
                Self::wallet_to_minor(request.amount_units)?,
                "USD",
                format!("pv-wallet-{}", uuid::Uuid::new_v4().simple()),
            )
            .await?;
        Ok(CheckoutDetails {
            provider_checkout_id: d.reference,
            checkout_url: d.authorization_url,
        })
    }
    async fn charge_saved_payment_method(
        &self,
        request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<PaymentVerification> {
        let auth = request
            .payment_method
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                    "saved payment authorization is required",
                ))
            })?;
        let email = request.customer_email.trim();
        if email.is_empty() {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other("customer email is required"),
            ));
        }
        let reference = format!("pv-auto-{}", request.idempotency_key);
        let r = self
            .auth(self.client.post(format!(
                "{}/transaction/charge_authorization",
                self.base_url
            )))
            .json(&ChargeAuthorizationRequest {
                email,
                amount: Self::wallet_to_minor(request.amount_units)?,
                authorization_code: auth,
                reference,
                currency: "USD",
            })
            .send()
            .await?;
        let status = r.status();
        let body: PaystackResponse<TransactionData> = r.json().await?;
        if !status.is_success() || !body.status {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other(
                    body.message
                        .unwrap_or_else(|| format!("Paystack HTTP {}", status)),
                ),
            ));
        }
        let tx = body.data.ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                "Paystack returned no charge data",
            ))
        })?;
        let units = Self::minor_to_wallet(tx.amount, &tx.currency)?;
        let currency = tx.currency.to_ascii_uppercase();
        let pid = tx.id.to_string();
        Ok(match tx.status.as_str() {
            "success" => PaymentVerification::Paid {
                provider_payment_id: pid,
                provider_reference: tx.reference,
                amount_units: units,
                currency,
            },
            "pending" | "ongoing" | "processing" | "queued" => PaymentVerification::Pending {
                provider_payment_id: Some(pid),
                provider_reference: Some(tx.reference),
                amount_units: Some(units),
                currency: Some(currency),
            },
            _ => PaymentVerification::Failed {
                provider_payment_id: Some(pid),
                provider_reference: Some(tx.reference),
                amount_units: Some(units),
                currency: Some(currency),
            },
        })
    }
    async fn refund_payment(
        &self,
        request: &RefundRequest,
    ) -> PaymentProviderResult<RefundDetails> {
        let r = self
            .auth(self.client.post(format!("{}/refund", self.base_url)))
            .json(&RefundApiRequest {
                transaction: &request.provider_payment_id,
                amount: Self::wallet_to_minor(request.amount_units)?,
                currency: &request.currency,
            })
            .send()
            .await?;
        let status = r.status();
        let body: PaystackResponse<RefundData> = r.json().await?;
        if !status.is_success() || !body.status {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                std::io::Error::other(
                    body.message
                        .unwrap_or_else(|| format!("Paystack HTTP {}", status)),
                ),
            ));
        }
        Ok(RefundDetails {
            provider_refund_id: body
                .data
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::other(
                        "Paystack returned no refund data",
                    ))
                })?
                .id
                .to_string(),
        })
    }
}

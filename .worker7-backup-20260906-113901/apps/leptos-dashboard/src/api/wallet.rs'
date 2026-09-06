use crate::api::client::{self, ApiError};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Wallet {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletTransaction {
    pub id: String,
    pub transaction_type: String,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}

pub async fn get() -> Result<Wallet, ApiError> {
    client::get_json("/wallet").await
}
pub async fn transactions() -> Result<Vec<WalletTransaction>, ApiError> {
    client::get_json("/wallet/transactions").await
}

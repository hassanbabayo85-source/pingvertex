//! Task 8 — PAYG retirement responses.
//!
//! All PAYG *mutation* endpoints return HTTP 410 with a stable
//! machine-readable `code: "payg_retired"`. Read-only wallet endpoints
//! (balance, transactions, notifications, settings GET) stay live so
//! customers can still see their historical balance and transaction log
//! during the transition.
//!
//! Historical data is preserved in the database; this handler only
//! changes the HTTP surface. `wallet_transactions`, `payments`, and the
//! immutability trigger are untouched.

use axum::Json;
use axum::http::StatusCode;
use serde_json::json;

/// Returns HTTP 410 Gone with `code: "payg_retired"`.
///
/// Mounted on every PAYG mutation endpoint instead of its former
/// handler. Authentication middleware still runs before this handler
/// (these routes live inside the JWT-protected billing/wallet routers),
/// so an unauthenticated caller gets 401 as before — the 410 only
/// appears once the caller has proven who they are.
pub async fn retired() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::GONE,
        Json(json!({
            "message": "PAYG has been retired. Please move to a subscription plan.",
            "code": "payg_retired",
            "guidance": "GET /billing/subscription to view your current plan; POST /billing/subscription/checkout to upgrade."
        })),
    )
}

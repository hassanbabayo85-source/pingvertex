use crate::error::ApiError;

#[allow(unused_variables)]
pub async fn update_subscription_usage() -> Result<(), ApiError> {
    Err(ApiError::gone("Subscription usage reporting is no longer supported under Pay-As-You-Go."))
}

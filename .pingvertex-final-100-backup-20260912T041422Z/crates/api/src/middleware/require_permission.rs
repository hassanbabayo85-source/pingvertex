use axum::{body::Body, http::Request, http::StatusCode, middleware::Next, response::Response};

use domain::shared::types::{AuthenticatedUser, Permission};

/// Axum middleware that requires the request to already contain an
/// `AuthenticatedUser` in extensions and for that user to have the given
/// `Permission`.
///
/// This middleware is intended to be layered AFTER `jwt_auth` (or any
/// other middleware that inserts `AuthenticatedUser`) on protected
/// routes. If no `AuthenticatedUser` is present, this is treated as
/// `UNAUTHORIZED`; if the user is present but lacks the permission, this
/// returns `FORBIDDEN`.
///
/// The permission is passed as a value at route-layer wiring time, e.g.
/// in the router:
///
/// ```ignore
/// .route_layer(from_fn(move |req, next| {
///     require_permission(Permission::CreateMonitor, req, next)
/// }))
/// ```
pub async fn require_permission(
    permission: Permission,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !user.can(permission) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}

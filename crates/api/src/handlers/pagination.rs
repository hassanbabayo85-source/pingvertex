//! Shared pagination plumbing for API handlers.
//!
//! Every list endpoint exposes the same query parameters and the same
//! `X-Next-Cursor` response header. Keeping these in one place means the
//! contract is defined once, not duplicated (and silently divergent)
//! across N handlers.
//!
//! * `limit` is clamped into `[1, MAX_PAGE_SIZE]` by `PageRequest::new`
//!   — no client can force an unbounded query with `?limit=1000000`.
//! * `cursor` is opaque; a malformed value starts from the first page
//!   (never leaks a distinguishable error), so an attacker cannot probe
//!   the encoding.
//! * `X-Next-Cursor` is present only when a next page exists; its
//!   absence is the signal that the client has reached the last page.

use axum::http::{HeaderMap, HeaderValue, header::HeaderName};
use serde::Deserialize;

use application::pagination::PageRequest;

use crate::error::ApiError;

/// Standard query parameters for every paginated list endpoint.
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

impl PaginationQuery {
    pub fn into_page_request(self) -> PageRequest {
        PageRequest::new(self.limit, self.cursor.as_deref())
    }
}

/// Response header name carrying the opaque cursor for the next page.
pub const NEXT_CURSOR_HEADER: HeaderName = HeaderName::from_static("x-next-cursor");

/// Build the outgoing `X-Next-Cursor` header (if `next_cursor` is
/// `Some`). Absence of the header = "last page".
pub fn pagination_headers(next_cursor: Option<String>) -> Result<HeaderMap, ApiError> {
    let mut headers = HeaderMap::new();
    if let Some(cursor) = next_cursor {
        let value = HeaderValue::from_str(&cursor)
            .map_err(|_| ApiError::internal("failed to encode next cursor"))?;
        headers.insert(NEXT_CURSOR_HEADER, value);
    }
    Ok(headers)
}

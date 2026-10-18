//! Shared cursor-based pagination primitives.
//!
//! Every tenant-scoped list endpoint in this codebase is bounded by
//! these types. The two invariants callers MUST rely on:
//!
//!   1. A client cannot force an unbounded `LIMIT`. `PageRequest::new`
//!      clamps every requested limit into `[1, MAX_PAGE_SIZE]`, and
//!      `DEFAULT_PAGE_SIZE` is used when the client omits `limit`.
//!   2. Pagination is stable across concurrent inserts/updates: the
//!      cursor sorts by `(created_at DESC, id DESC)`, so a new row
//!      inserted between two pages can never cause an already-seen row
//!      to be returned twice, nor a row to be skipped.
//!
//! The cursor is deliberately OPAQUE: clients must treat it as a black
//! box and echo it back unchanged. Its encoding (a plain
//! `"v1:{micros}:{uuid}"` string) is an implementation detail and may
//! change without notice — do not parse it in client code.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Default page size when the client does not supply `limit`.
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// Hard cap on page size. Any client-supplied `limit` above this is
/// clamped, never rejected — an attacker asking for `limit=1000000`
/// gets `limit=100`, and a well-behaved client asking for slightly too
/// much gets its request served, just smaller.
pub const MAX_PAGE_SIZE: u32 = 100;

/// Opaque cursor over `(created_at, id)`.
///
/// `created_at` is second-or-finer resolution (Postgres `TIMESTAMPTZ`);
/// `id` is the tiebreaker for rows created in the same instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

impl Cursor {
    pub fn new(created_at: DateTime<Utc>, id: Uuid) -> Self {
        Self { created_at, id }
    }

    /// Encode into the opaque string clients echo back.
    ///
    /// The `v1:` prefix is a version tag: if the encoding ever needs to
    /// change, old cursors (`v1:...`) can still be decoded or explicitly
    /// rejected by version, instead of silently mis-parsed.
    pub fn encode(&self) -> String {
        format!("v1:{}:{}", self.created_at.timestamp_micros(), self.id)
    }

    /// Decode a previously-issued cursor.
    ///
    /// Returns `None` for ANY malformed input (missing prefix, bad
    /// integer, invalid UUID, out-of-range timestamp). Callers MUST
    /// treat `None` as "ignore the cursor and start from the first
    /// page" rather than as an error — leaking "your cursor is
    /// malformed" would let an attacker probe the encoding.
    pub fn decode(raw: &str) -> Option<Self> {
        let rest = raw.strip_prefix("v1:")?;
        let (micros_str, uuid_str) = rest.split_once(':')?;
        let micros: i64 = micros_str.parse().ok()?;
        let created_at = DateTime::<Utc>::from_timestamp_micros(micros)?;
        let id = Uuid::parse_str(uuid_str).ok()?;
        Some(Self { created_at, id })
    }
}

/// A validated, bounded page request.
#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    pub limit: u32,
    pub cursor: Option<Cursor>,
}

impl PageRequest {
    /// Build a `PageRequest` from raw client input.
    ///
    /// * `limit`: `None` uses `DEFAULT_PAGE_SIZE`; anything above
    ///   `MAX_PAGE_SIZE` is clamped. Zero is clamped to `1` so a
    ///   `limit=0` request never becomes a division-by-zero / infinite
    ///   loop hazard for callers.
    /// * `cursor`: a malformed cursor is treated as `None`, meaning
    ///   "start from the beginning" — see `Cursor::decode`.
    pub fn new(limit: Option<u32>, cursor: Option<&str>) -> Self {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
        let cursor = cursor.and_then(Cursor::decode);
        Self { limit, cursor }
    }

    /// Convenience: build a first-page request with `DEFAULT_PAGE_SIZE`.
    pub fn first() -> Self {
        Self {
            limit: DEFAULT_PAGE_SIZE,
            cursor: None,
        }
    }

    /// The number of rows the SQL layer should actually fetch.
    ///
    /// Repositories fetch `limit + 1` rows: if `limit + 1` come back,
    /// there is a next page, and the extra row is used only to detect
    /// that fact (then dropped). This is the standard "peek one ahead"
    /// technique that gives deterministic `next_cursor` without a
    /// second query.
    pub fn fetch_size(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

/// One page of results, plus the cursor for the next page (if any).
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// `None` iff there is no next page. Otherwise an opaque string the
    /// client echoes back via `?cursor=...`.
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Build a `Page<T>` from the `limit + 1` rows the repository
    /// fetched.
    ///
    /// `cursor_of` maps each item to its `(created_at, id)` cursor —
    /// usually `|item| Cursor::new(item.created_at, item.id.0)`.
    pub fn from_rows<F>(mut rows: Vec<T>, limit: u32, cursor_of: F) -> Self
    where
        F: Fn(&T) -> Cursor,
    {
        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            rows.last().map(|row| cursor_of(row).encode())
        } else {
            None
        };
        Self {
            items: rows,
            next_cursor,
        }
    }

    /// Convenience for empty results.
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
        }
    }
}

/// Filter an in-memory collection according to `page` and produce a
/// `Page`.
///
/// Used by `InMemory*` repository implementations and as the DEFAULT
/// trait-method body for every port that has not (yet) grown a
/// SQL-level cursor-paginated override. Sorting here matches the SQL
/// ordering (`created_at DESC, id DESC`) so switching a repository from
/// in-memory to Postgres does not change pagination results.
pub fn paginate_in_memory<T, F>(mut items: Vec<T>, page: PageRequest, cursor_of: F) -> Page<T>
where
    F: Fn(&T) -> Cursor,
{
    items.sort_by(|a, b| {
        let ca = cursor_of(a);
        let cb = cursor_of(b);
        cb.created_at
            .cmp(&ca.created_at)
            .then_with(|| cb.id.cmp(&ca.id))
    });

    if let Some(c) = page.cursor {
        items.retain(|item| {
            let cur = cursor_of(item);
            cur.created_at < c.created_at || (cur.created_at == c.created_at && cur.id < c.id)
        });
    }

    items.truncate(page.fetch_size() as usize);
    Page::from_rows(items, page.limit, cursor_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_request_defaults_to_default_page_size() {
        let req = PageRequest::new(None, None);
        assert_eq!(req.limit, DEFAULT_PAGE_SIZE);
        assert!(req.cursor.is_none());
    }

    #[test]
    fn page_request_clamps_oversized_limit() {
        let req = PageRequest::new(Some(1_000_000), None);
        assert_eq!(
            req.limit, MAX_PAGE_SIZE,
            "a client-supplied limit must never exceed MAX_PAGE_SIZE"
        );
    }

    #[test]
    fn page_request_clamps_zero_limit_to_one() {
        let req = PageRequest::new(Some(0), None);
        assert_eq!(req.limit, 1);
    }

    #[test]
    fn page_request_accepts_a_limit_below_max() {
        let req = PageRequest::new(Some(25), None);
        assert_eq!(req.limit, 25);
    }

    #[test]
    fn page_request_ignores_a_malformed_cursor() {
        let req = PageRequest::new(Some(10), Some("not-a-real-cursor"));
        assert!(
            req.cursor.is_none(),
            "malformed cursor must start from page 1"
        );
    }

    #[test]
    fn cursor_round_trips() {
        let original = Cursor::new(
            DateTime::<Utc>::from_timestamp(1_700_000_000, 500_000_000).unwrap(),
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
        );
        let encoded = original.encode();
        let decoded = Cursor::decode(&encoded).expect("must decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn cursor_decode_rejects_garbage() {
        for bad in [
            "",
            "v2:123:550e8400-e29b-41d4-a716-446655440000", // wrong version
            "v1:not-a-number:550e8400-e29b-41d4-a716-446655440000",
            "v1:1700000000000000:not-a-uuid",
            "v1:1700000000000000", // missing uuid
        ] {
            assert!(
                Cursor::decode(bad).is_none(),
                "cursor {bad:?} must not decode"
            );
        }
    }

    #[test]
    fn page_from_rows_emits_next_cursor_when_more_than_limit() {
        #[derive(Clone)]
        #[allow(dead_code)]
        struct Row(i32, DateTime<Utc>, Uuid);

        let base = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let rows: Vec<Row> = (0..3)
            .map(|i| Row(i, base - chrono::Duration::seconds(i as i64), Uuid::nil()))
            .collect();

        let page = Page::from_rows(rows, 2, |r| Cursor::new(r.1, r.2));
        assert_eq!(page.items.len(), 2);
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn page_from_rows_emits_no_next_cursor_at_end() {
        #[derive(Clone)]
        struct Row(DateTime<Utc>, Uuid);
        let base = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let rows = vec![Row(base, Uuid::nil()), Row(base, Uuid::nil())];
        let page = Page::from_rows(rows, 2, |r| Cursor::new(r.0, r.1));
        assert_eq!(page.items.len(), 2);
        assert!(page.next_cursor.is_none());
    }
}

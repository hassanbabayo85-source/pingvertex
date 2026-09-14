//! Shared URL/path sanitization for RUM entities.
//!
//! SECURITY INVARIANT: browser telemetry must never become a secondary
//! secret/PII storage channel.
//!
//! Every RUM field that carries a URL-shaped value — page path,
//! resource URL, JS error source — is passed through
//! [`strip_query_and_fragment`] before it is persisted. This means a
//! `?token=…`, `?code=…`, `?password=…`, `?secret=…`, `?key=…`,
//! `?email=…` (or *any* other query parameter) and *any* `#fragment`
//! can never reach the database, regardless of what the client SDK did
//! or did not strip.
//!
//! Scope: this function strips only the query string and the fragment.
//! The `scheme://host/path` prefix (or the bare path, for relative
//! inputs) is preserved verbatim. Userinfo (`user:pass@`) is NOT
//! stripped — the SDK never emits it, and adding URL parsing here would
//! introduce failure modes for malformed input without buying anything
//! for the stated threat model (secret-bearing query parameters).

/// Strips the query string and the fragment from a URL or path.
///
/// Returns the input truncated at the first `?` or `#`, whichever
/// appears first. The scheme/host/path prefix is preserved verbatim
/// (this function does not parse or re-serialize the URL — it only
/// guarantees the result contains neither `?` nor `#`).
///
/// Examples:
///
/// * `/page?token=secret`                      -> `/page`
/// * `https://example.com/page?token=secret#f` -> `https://example.com/page`
/// * `/page#frag?token=secret`                 -> `/page`
/// * `https://example.com/page`                -> `https://example.com/page`
pub fn strip_query_and_fragment(input: &str) -> String {
    match input.find(|c| c == '?' || c == '#') {
        Some(idx) => input[..idx].to_string(),
        None => input.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_query_string() {
        assert_eq!(strip_query_and_fragment("/page?token=secret"), "/page");
    }

    #[test]
    fn strips_fragment() {
        assert_eq!(strip_query_and_fragment("/page#section"), "/page");
    }

    #[test]
    fn strips_query_and_fragment_at_first_boundary() {
        assert_eq!(
            strip_query_and_fragment("https://example.com/page?token=secret#fragment"),
            "https://example.com/page"
        );
    }

    #[test]
    fn fragment_before_query_still_cuts_at_fragment() {
        assert_eq!(
            strip_query_and_fragment("/page#frag?token=secret"),
            "/page"
        );
    }

    #[test]
    fn clean_url_passes_through_unchanged() {
        assert_eq!(
            strip_query_and_fragment("https://example.com/page"),
            "https://example.com/page"
        );
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(strip_query_and_fragment(""), "");
    }

    #[test]
    fn query_only_input_becomes_empty() {
        assert_eq!(strip_query_and_fragment("?token=secret"), "");
    }

    #[test]
    fn fragment_only_input_becomes_empty() {
        assert_eq!(strip_query_and_fragment("#frag"), "");
    }

    #[test]
    fn every_named_secret_param_is_removed() {
        for param in [
            "?token=t",
            "?code=c",
            "?password=p",
            "?secret=s",
            "?key=k",
            "?email=e",
        ] {
            let input = format!("https://example.com/page{param}");
            let out = strip_query_and_fragment(&input);
            assert_eq!(out, "https://example.com/page");
            assert!(!out.contains('?'));
            assert!(!out.contains('#'));
        }
    }
}

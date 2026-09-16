//! PART 2 — Proxy input & import.
//!
//! This module layers an "import summary" on top of the existing
//! `proxy_list_parser` primitives (`parse_csv_line` and friends). It exists
//! because the original parser is fail-fast: it returns `Err` on the FIRST
//! invalid line, which means one typo in a 500-line CSV aborts the whole
//! batch. That is hostile to any bulk-import workflow.
//!
//! Instead of replacing the original parser, this module ADDS a
//! summary-aware path used by the preview endpoint and dashboard:
//!
//!   parse_csv_with_summary(input)  -> Ok(ProxyImportSummary)
//!   parse_json_with_summary(input) -> Ok(ProxyImportSummary)
//!
//! Both functions:
//!   * never abort on a single bad line — they classify every input unit
//!     (line for CSV, array element for JSON) and keep going,
//!   * normalize each valid entry (lowercase scheme/host, strip IPv6
//!     brackets, drop empty credentials),
//!   * run domain-level validation (`ProxyHost::new` + port allowlist)
//!     so that private IPs, disallowed ports and `localhost` are rejected
//!     here instead of at worker time,
//!   * detect duplicates within the SAME batch by a canonical key, so
//!     `1.2.3.4:8080` and `HTTP://1.2.3.4:8080` collapse to one entry,
//!   * return counts in a single struct so the UI can render a summary
//!     without re-scanning the input.
//!
//! The original `parse_csv` / `parse_json` remain untouched for backward
//! compatibility — they still fail fast, still emit the same error
//! strings, and every existing caller keeps working.

use std::collections::HashMap;

use domain::ProxyScheme;
use domain::value_objects::{ProxyHost, is_allowed_proxy_port};

use crate::dto::ProxyEntryInput;

use super::proxy_list_parser::{MAX_CSV_LINES, MAX_JSON_BYTES, parse_csv_line};

/// Upper bound on how much of a raw input line is stored in a
/// `RejectedProxy` / `DuplicateProxy` record. Keeps the summary bounded
/// even if a single line is pathological (e.g. 1 MB of `a`s).
const MAX_RAW_SNIPPET_BYTES: usize = 256;

/// The outcome of parsing a proxy batch. Every input unit (CSV line,
/// JSON array element) lands in exactly one bucket:
///
/// - `accepted`   — passed shape parsing + domain validation, unique in batch
/// - `duplicates` — passed validation but matched an earlier entry's dedup key
/// - `rejected`   — failed shape parsing OR domain validation (both are
///                  reported here with a human-readable `reason`; `invalid`
///                  is a COUNT of the shape-parse subset for convenience)
///
/// Invariant after a successful parse:
///   `total == invalid + rejected.len() + valid`
/// where `valid == accepted.len() + duplicates.len()`.
#[derive(Debug, Clone, Default)]
pub struct ProxyImportSummary {
    /// Total non-empty, non-comment input units.
    pub total: usize,
    /// Number of shape-parse failures (bad `host:port`, unknown scheme,
    /// non-numeric port, etc.). These ALSO appear in `rejected` so the UI
    /// has a single list to display.
    pub invalid: usize,
    /// Valid entries that matched an earlier entry's dedup key.
    pub duplicates: Vec<DuplicateProxy>,
    /// Every entry that failed — shape-parse OR domain validation.
    pub rejected: Vec<RejectedProxy>,
    /// Valid, unique entries — what should actually be submitted.
    ///
    /// Each entry carries the 1-based `position` it came from in the
    /// input (CSV line number or JSON array index), so callers can map
    /// an accepted entry back to the original line without re-parsing.
    pub accepted: Vec<AcceptedProxy>,
}

impl ProxyImportSummary {
    /// `accepted.len() + duplicates.len()`.
    pub fn valid_count(&self) -> usize {
        self.accepted.len() + self.duplicates.len()
    }

    /// Alias for `duplicates.len()` — matches the naming used in the UI.
    pub fn duplicate_count(&self) -> usize {
        self.duplicates.len()
    }

    /// Alias for `rejected.len()` — matches the naming used in the UI.
    pub fn rejected_count(&self) -> usize {
        self.rejected.len()
    }
}

/// One accepted entry, together with the 1-based position it came from
/// in the input. The position lets a caller map an entry back to the
/// exact CSV line or JSON array index that produced it — which is how
/// the dashboard filters the ORIGINAL parsed inputs (which still carry
/// credentials) down to just the accepted ones before submitting, since
/// this summary deliberately does not carry credentials itself.
#[derive(Debug, Clone)]
pub struct AcceptedProxy {
    pub position: usize,
    pub input: ProxyEntryInput,
}

/// One entry that failed parsing or validation. `position` is 1-based:
/// it is the CSV line number for CSV input, the array index for JSON.
#[derive(Debug, Clone)]
pub struct RejectedProxy {
    pub position: usize,
    pub raw: String,
    pub reason: String,
}

/// One entry that duplicates an earlier entry in the same batch.
#[derive(Debug, Clone)]
pub struct DuplicateProxy {
    /// Position of the LATER (duplicate) entry.
    pub position: usize,
    pub raw: String,
    /// Position of the FIRST entry that used the same dedup key.
    pub first_seen_at: usize,
}

// ---------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------

/// Summary-aware CSV parser. Never aborts on a single bad line — the
/// batch is always processed in full. Returns `Err` only for structural
/// problems that make the whole input unusable:
///   * the line count exceeds `MAX_CSV_LINES`,
///   * the input contains zero non-empty, non-comment lines.
pub fn parse_csv_with_summary(input: &str) -> Result<ProxyImportSummary, String> {
    // Collect non-empty, non-comment lines with their original 1-based
    // line numbers preserved (so the summary can point at the right
    // place even when blank lines and comments are scattered through).
    let lines: Vec<(usize, &str)> = input
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .collect();

    if lines.is_empty() {
        return Err("CSV contains no proxies".to_string());
    }

    if lines.len() > MAX_CSV_LINES {
        return Err(format!(
            "CSV contains {} proxy lines; maximum allowed is {}",
            lines.len(),
            MAX_CSV_LINES
        ));
    }

    let mut summary = ProxyImportSummary {
        total: lines.len(),
        ..Default::default()
    };
    let mut seen: HashMap<DedupKey, usize> = HashMap::new();

    for (position, line) in lines {
        // Step 1 — shape parsing (reuses the original fail-fast parser
        // line-by-line, so the two code paths can never drift apart on
        // what counts as a well-formed entry).
        let parsed = match parse_csv_line(line) {
            Ok(p) => p,
            Err(reason) => {
                summary.invalid += 1;
                summary.rejected.push(RejectedProxy {
                    position,
                    raw: truncate_raw(line),
                    reason,
                });
                continue;
            }
        };

        // Step 2 — normalization.
        let normalized = normalize_input(parsed);

        // Step 3 — domain validation.
        if let Err(reason) = validate_against_domain(&normalized) {
            summary.rejected.push(RejectedProxy {
                position,
                raw: truncate_raw(line),
                reason,
            });
            continue;
        }

        // Step 4 — duplicate detection within the batch.
        let key = dedup_key(&normalized);
        if let Some(&first_seen_at) = seen.get(&key) {
            summary.duplicates.push(DuplicateProxy {
                position,
                raw: truncate_raw(line),
                first_seen_at,
            });
            continue;
        }
        seen.insert(key, position);
        summary.accepted.push(AcceptedProxy {
            position,
            input: normalized,
        });
    }

    Ok(summary)
}

/// Summary-aware JSON parser. Accepts the same two shapes as the
/// original `parse_json` (bare array, or `{ "proxies": [...] }`). Each
/// array element is deserialized INDIVIDUALLY, so one malformed object
/// does not poison the whole batch.
pub fn parse_json_with_summary(input: &str) -> Result<ProxyImportSummary, String> {
    if input.len() > MAX_JSON_BYTES {
        return Err(format!(
            "JSON input size {} bytes exceeds maximum allowed {} bytes",
            input.len(),
            MAX_JSON_BYTES
        ));
    }

    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("invalid JSON: {e}"))?;

    // Unwrap `{ "proxies": [...] }` into the bare array; leave a bare
    // array as-is. Anything else is a structural error.
    let entries_value = match value {
        serde_json::Value::Array(_) => value,
        serde_json::Value::Object(mut object) => object
            .remove("proxies")
            .ok_or_else(|| "JSON body must contain a 'proxies' array".to_string())?,
        _ => {
            return Err(
                "JSON body must be an array or an object containing 'proxies'".to_string(),
            );
        }
    };

    let entries_array = match entries_value {
        serde_json::Value::Array(array) => array,
        _ => {
            return Err("JSON 'proxies' field must be an array".to_string());
        }
    };

    if entries_array.is_empty() {
        return Err("JSON contains no proxies".to_string());
    }

    if entries_array.len() > MAX_CSV_LINES {
        return Err(format!(
            "JSON array contains {} proxies; maximum allowed is {}",
            entries_array.len(),
            MAX_CSV_LINES
        ));
    }

    let mut summary = ProxyImportSummary {
        total: entries_array.len(),
        ..Default::default()
    };
    let mut seen: HashMap<DedupKey, usize> = HashMap::new();

    for (idx, entry_value) in entries_array.into_iter().enumerate() {
        let position = idx + 1;
        let raw = truncate_raw(&entry_value.to_string());

        // Step 1 — per-entry deserialization. A bad shape here is
        // recorded as `invalid` (and also in `rejected`), but does not
        // abort the batch.
        let entry: ProxyEntryInput = match serde_json::from_value(entry_value) {
            Ok(e) => e,
            Err(err) => {
                summary.invalid += 1;
                summary.rejected.push(RejectedProxy {
                    position,
                    raw,
                    reason: format!("invalid entry shape: {err}"),
                });
                continue;
            }
        };

        // Steps 2–4 — normalization, validation, dedup (identical to CSV).
        let normalized = normalize_input(entry);

        if let Err(reason) = validate_against_domain(&normalized) {
            summary.rejected.push(RejectedProxy {
                position,
                raw,
                reason,
            });
            continue;
        }

        let key = dedup_key(&normalized);
        if let Some(&first_seen_at) = seen.get(&key) {
            summary.duplicates.push(DuplicateProxy {
                position,
                raw,
                first_seen_at,
            });
            continue;
        }
        seen.insert(key, position);
        summary.accepted.push(AcceptedProxy {
            position,
            input: normalized,
        });
    }

    Ok(summary)
}

// ---------------------------------------------------------------
// Internals
// ---------------------------------------------------------------

/// Canonical key used to detect duplicates inside a single batch.
/// Credentials are part of the key: `1.2.3.4:8080` and
/// `1.2.3.4:8080:user:pass` are two different proxies, because they will
/// be dialed with different authentication.
type DedupKey = (String, String, u16, Option<String>, Option<String>);

fn scheme_str(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks4 => "socks4",
        ProxyScheme::Socks5 => "socks5",
    }
}

/// Canonicalizes an entry in place: lowercase scheme name and host,
/// strip IPv6 brackets, trim credentials, drop empty credential fields.
/// (Scheme is a typed enum so there is nothing to lowercase on it.)
fn normalize_input(mut input: ProxyEntryInput) -> ProxyEntryInput {
    let host = input.host.trim();
    // Bracketed IPv6 ("[::1]") is accepted in some inputs; strip the
    // brackets so the canonical form matches what ProxyHost expects.
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    input.host = host.to_ascii_lowercase();

    input.username = input
        .username
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    input.password = input
        .password
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());

    input
}

/// Domain-level validation: rejects anything `ProxyHost::new` refuses
/// (invalid hostname syntax, private/reserved literal IPs, hostname too
/// long) and anything outside the proxy port allowlist. Also rejects
/// `localhost` explicitly, because `ProxyHost::new` only validates
/// literal IPs — a hostname that RESOLVES to loopback would still be
/// rejected by the worker's SSRF check, but rejecting it here gives the
/// user a clear error on preview instead of a mysterious failure later.
fn validate_against_domain(input: &ProxyEntryInput) -> Result<(), String> {
    if input.host.eq_ignore_ascii_case("localhost")
        || input.host.to_ascii_lowercase().ends_with(".localhost")
    {
        return Err("localhost is not an allowed proxy host".to_string());
    }

    ProxyHost::new(input.host.clone()).map_err(|e| format!("invalid host: {e}"))?;

    if !is_allowed_proxy_port(input.port) {
        return Err(format!(
            "port {} is not an allowed proxy port",
            input.port
        ));
    }

    Ok(())
}

fn dedup_key(input: &ProxyEntryInput) -> DedupKey {
    (
        scheme_str(&input.scheme).to_string(),
        input.host.clone(),
        input.port,
        input.username.clone(),
        input.password.clone(),
    )
}

fn truncate_raw(raw: &str) -> String {
    if raw.len() <= MAX_RAW_SNIPPET_BYTES {
        return raw.to_string();
    }
    // Find a char boundary so we never slice inside a multi-byte codepoint.
    let boundary = raw
        .char_indices()
        .nth(MAX_RAW_SNIPPET_BYTES)
        .map(|(i, _)| i)
        .unwrap_or(raw.len());
    format!("{}…", &raw[..boundary])
}

// ---------------------------------------------------------------
// Tests
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- CSV: happy path & summary counts --

    #[test]
    fn csv_all_valid_accepts_everything() {
        let summary = parse_csv_with_summary(
            "1.2.3.4:8080\n1.2.3.5:8081\nsocks5://8.8.8.8:1080\n",
        )
        .unwrap();

        assert_eq!(summary.total, 3);
        assert_eq!(summary.invalid, 0);
        assert_eq!(summary.rejected_count(), 0);
        assert_eq!(summary.duplicate_count(), 0);
        assert_eq!(summary.valid_count(), 3);
        assert_eq!(summary.accepted.len(), 3);
    }

    #[test]
    fn csv_invalid_line_does_not_abort_batch() {
        // The middle line has a non-numeric port. The first and third
        // lines must still be accepted — that is the whole point of
        // this module.
        let summary = parse_csv_with_summary(
            "1.2.3.4:8080\n1.2.3.5:not-a-port\n8.8.8.8:443\n",
        )
        .unwrap();

        assert_eq!(summary.total, 3);
        assert_eq!(summary.invalid, 1);
        assert_eq!(summary.rejected_count(), 1);
        assert_eq!(summary.accepted.len(), 2);
        assert_eq!(summary.accepted[0].input.host, "1.2.3.4");
        assert_eq!(summary.accepted[1].input.host, "8.8.8.8");
        assert_eq!(summary.rejected[0].position, 2);
        assert!(summary.rejected[0].reason.contains("invalid port"));
    }

    #[test]
    fn csv_duplicate_detection_collapses_identical_lines() {
        let summary = parse_csv_with_summary(
            "1.2.3.4:8080\n1.2.3.4:8080\n1.2.3.4:8080\n",
        )
        .unwrap();

        assert_eq!(summary.total, 3);
        assert_eq!(summary.accepted.len(), 1);
        assert_eq!(summary.duplicate_count(), 2);
        assert_eq!(summary.duplicates[0].position, 2);
        assert_eq!(summary.duplicates[0].first_seen_at, 1);
        assert_eq!(summary.duplicates[1].position, 3);
        assert_eq!(summary.duplicates[1].first_seen_at, 1);
    }

    #[test]
    fn csv_normalization_makes_case_and_scheme_equivalent_lines_dedup() {
        // All four lines refer to the same proxy once normalized:
        //  * HTTP://1.2.3.4:8080   -> lowercased scheme, lowercased host
        //  * http://1.2.3.4:8080   -> already canonical
        //  * 1.2.3.4:8080          -> default http scheme
        //  * 1.2.3.4:8080          -> exact duplicate
        let summary = parse_csv_with_summary(
            "HTTP://1.2.3.4:8080\nhttp://1.2.3.4:8080\n1.2.3.4:8080\n1.2.3.4:8080\n",
        )
        .unwrap();

        assert_eq!(summary.accepted.len(), 1);
        assert_eq!(summary.duplicate_count(), 3);
    }

    #[test]
    fn csv_credentials_are_part_of_the_dedup_key() {
        // Same host:port but different credentials must NOT be treated
        // as duplicates — they are two distinct proxies.
        let summary = parse_csv_with_summary(
            "1.2.3.4:8080:alice:secret\n1.2.3.4:8080:bob:hunter2\n1.2.3.4:8080\n",
        )
        .unwrap();

        assert_eq!(summary.accepted.len(), 3);
        assert_eq!(summary.duplicate_count(), 0);
    }

    #[test]
    fn csv_ipv6_brackets_are_stripped_during_normalization() {
        let summary = parse_csv_with_summary("[2001:4860:4860::8888]:8080\n").unwrap();

        assert_eq!(summary.accepted.len(), 1);
        // Canonical host has no brackets.
        assert_eq!(summary.accepted[0].input.host, "2001:4860:4860::8888");
    }

    #[test]
    fn csv_private_ip_is_rejected_not_accepted() {
        let summary =
            parse_csv_with_summary("10.0.0.1:8080\n192.168.1.1:8080\n8.8.8.8:8080\n").unwrap();

        assert_eq!(summary.rejected_count(), 2);
        assert_eq!(summary.accepted.len(), 1);
        assert_eq!(summary.accepted[0].input.host, "8.8.8.8");
    }

    #[test]
    fn csv_localhost_is_rejected() {
        let summary =
            parse_csv_with_summary("localhost:8080\nfoo.localhost:8080\n").unwrap();

        assert_eq!(summary.rejected_count(), 2);
        assert!(summary.rejected[0].reason.contains("localhost"));
    }

    #[test]
    fn csv_disallowed_port_is_rejected() {
        // Port 22 (SSH) is not on the proxy-port allowlist.
        let summary = parse_csv_with_summary("1.2.3.4:22\n").unwrap();

        assert_eq!(summary.rejected_count(), 1);
        assert!(summary.rejected[0].reason.contains("not an allowed proxy port"));
    }

    #[test]
    fn csv_mixed_batch_reports_every_bucket() {
        // 2 valid + 1 duplicate + 1 shape invalid + 1 domain rejected
        let summary = parse_csv_with_summary(
            "\
1.2.3.4:8080
1.2.3.5:443
1.2.3.4:8080
not-a-proxy
10.0.0.1:8080
",
        )
        .unwrap();

        assert_eq!(summary.total, 5);
        assert_eq!(summary.accepted.len(), 2);
        assert_eq!(summary.duplicate_count(), 1);
        assert_eq!(summary.invalid, 1);
        // rejected includes the shape-invalid line AND the domain-rejected line
        assert_eq!(summary.rejected_count(), 2);
        assert_eq!(summary.valid_count(), 3);
    }

    #[test]
    fn csv_rejects_more_than_500_lines_after_filtering_blanks() {
        // 501 non-empty lines should trip the limit even if blank lines
        // and comments are interleaved.
        let mut input = String::new();
        for _ in 0..501 {
            input.push_str("1.2.3.4:8080\n# comment\n\n");
        }
        let err = parse_csv_with_summary(&input).unwrap_err();
        assert!(err.contains("maximum allowed is 500"));
    }

    #[test]
    fn csv_empty_input_is_structural_error() {
        assert!(parse_csv_with_summary("").is_err());
        assert!(parse_csv_with_summary("\n\n# only comments\n").is_err());
    }

    #[test]
    fn csv_preserves_original_line_numbers_when_blanks_are_present() {
        let summary = parse_csv_with_summary(
            "\
# comment
1.2.3.4:8080

garbage
",
        )
        .unwrap();

        // The garbage line is the 4th line of the file, but the 2nd
        // non-empty, non-comment unit — the summary must report 4.
        assert_eq!(summary.rejected[0].position, 4);
    }

    // -- JSON --

    #[test]
    fn json_bare_array_all_valid() {
        let input = r#"[
            {"host":"1.2.3.4","port":8080,"scheme":"Http"},
            {"host":"8.8.8.8","port":443,"scheme":"Https"}
        ]"#;

        let summary = parse_json_with_summary(input).unwrap();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.accepted.len(), 2);
        assert_eq!(summary.invalid, 0);
    }

    #[test]
    fn json_object_with_proxies_field_works() {
        let input = r#"{"proxies":[{"host":"1.2.3.4","port":8080,"scheme":"Http"}]}"#;
        let summary = parse_json_with_summary(input).unwrap();
        assert_eq!(summary.accepted.len(), 1);
    }

    #[test]
    fn json_malformed_entry_does_not_abort_batch() {
        // The middle object is missing `port`. The first and third must
        // still be accepted.
        let input = r#"[
            {"host":"1.2.3.4","port":8080,"scheme":"Http"},
            {"host":"1.2.3.5","scheme":"Http"},
            {"host":"8.8.8.8","port":443,"scheme":"Https"}
        ]"#;

        let summary = parse_json_with_summary(input).unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.invalid, 1);
        assert_eq!(summary.accepted.len(), 2);
        assert_eq!(summary.rejected[0].position, 2);
    }

    #[test]
    fn json_duplicate_entries_are_detected() {
        let input = r#"[
            {"host":"1.2.3.4","port":8080,"scheme":"Http"},
            {"host":"1.2.3.4","port":8080,"scheme":"Http"},
            {"host":"1.2.3.4","port":8080,"scheme":"Http"}
        ]"#;

        let summary = parse_json_with_summary(input).unwrap();
        assert_eq!(summary.accepted.len(), 1);
        assert_eq!(summary.duplicate_count(), 2);
    }

    #[test]
    fn json_rejects_more_than_500_entries() {
        let entries: Vec<serde_json::Value> = (0..501)
            .map(|_| serde_json::json!({"host":"1.2.3.4","port":8080,"scheme":"Http"}))
            .collect();
        let input = serde_json::to_string(&entries).unwrap();
        let err = parse_json_with_summary(&input).unwrap_err();
        assert!(err.contains("maximum allowed is 500"));
    }

    #[test]
    fn json_rejects_oversized_payload() {
        let huge = "x".repeat(MAX_JSON_BYTES + 1);
        let err = parse_json_with_summary(&huge).unwrap_err();
        assert!(err.contains("exceeds maximum allowed"));
    }

    #[test]
    fn json_rejects_object_without_proxies_field() {
        let err = parse_json_with_summary(r#"{"not_proxies":[]}"#).unwrap_err();
        assert!(err.contains("must contain a 'proxies' array"));
    }

    #[test]
    fn json_empty_array_is_structural_error() {
        assert!(parse_json_with_summary("[]").is_err());
    }

    #[test]
    fn json_private_ip_is_rejected_but_batch_continues() {
        let input = r#"[
            {"host":"10.0.0.1","port":8080,"scheme":"Http"},
            {"host":"8.8.8.8","port":8080,"scheme":"Http"}
        ]"#;

        let summary = parse_json_with_summary(input).unwrap();
        assert_eq!(summary.accepted.len(), 1);
        assert_eq!(summary.rejected_count(), 1);
        assert_eq!(summary.accepted[0].input.host, "8.8.8.8");
    }

    // -- raw snippet truncation --

    #[test]
    fn raw_snippet_is_truncated_for_pathological_lines() {
        let long_line = format!("{}:8080", "a".repeat(10_000));
        let summary = parse_csv_with_summary(&long_line).unwrap();
        assert_eq!(summary.rejected_count(), 1);
        // 256-byte cap plus the single-byte ellipsis.
        assert!(summary.rejected[0].raw.len() <= MAX_RAW_SNIPPET_BYTES + 4);
    }
}

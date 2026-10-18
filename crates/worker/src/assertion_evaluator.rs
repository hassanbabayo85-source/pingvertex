use std::collections::HashMap;

use crate::regex_cache::{self, RegexCache};
use domain::entities::Assertion;
use domain::enums::{AssertionOperator, ResponsePart};
use domain::shared::ids::AssertionId;
use domain::value_objects::AssertionEvaluationResult;
use serde_json::Value;
use tracing::warn;

/// The parts extracted from the HTTP response that assertions are evaluated against.
pub struct EvalContext {
    pub status_code: u16,
    /// Every key is lowercased.
    pub headers: HashMap<String, String>,
    pub body_bytes: Vec<u8>,
    pub latency_ms: u128,
}

/// Evaluates all of a monitor's assertions against the response received.
/// Returns `(all_passed, results)`.
pub fn evaluate_assertions(
    assertions: &[Assertion],
    ctx: &EvalContext,
) -> (bool, Vec<AssertionEvaluationResult>) {
    evaluate_assertions_with_cache(assertions, ctx, regex_cache::global())
}

/// Evaluation entry point that lets the caller supply a specific
/// [`RegexCache`]. Production callers use [`evaluate_assertions`] (which
/// goes through the process-wide cache); tests use this variant so they can
/// assert on per-cache statistics without disturbing the global cache.
pub fn evaluate_assertions_with_cache(
    assertions: &[Assertion],
    ctx: &EvalContext,
    regex_cache: &RegexCache,
) -> (bool, Vec<AssertionEvaluationResult>) {
    let mut all_passed = true;
    let mut results = Vec::with_capacity(assertions.len());

    // If any assertion needs the JSON body, parse it exactly once.
    let mut json_cache: Option<Value> = None;

    for assertion in assertions {
        let result = evaluate_one(assertion, ctx, &mut json_cache, regex_cache);
        if !result.passed {
            all_passed = false;
            warn!(
                assertion_id = %assertion.id,
                message = ?result.message,
                "Tantancewar assertion ta gaza"
            );
        }
        results.push(result);
    }

    (all_passed, results)
}

fn evaluate_one(
    assertion: &Assertion,
    ctx: &EvalContext,
    json_cache: &mut Option<Value>,
    regex_cache: &RegexCache,
) -> AssertionEvaluationResult {
    // 1. Extract the actual value to test.
    let actual: Option<String> = match &assertion.response_part {
        ResponsePart::StatusCode => Some(ctx.status_code.to_string()),

        // Look the header up by name (case-insensitive). If the header
        // exists but its value is empty (""), `Exists` must still
        // succeed — that is guaranteed below by using
        // `contains_key` rather than relying on `Option<String>` alone.
        ResponsePart::Headers => assertion
            .target
            .as_ref()
            .and_then(|t| ctx.headers.get(&t.to_lowercase()).cloned()),

        ResponsePart::Body => String::from_utf8(ctx.body_bytes.clone())
            .ok()
            .or_else(|| Some(String::from_utf8_lossy(&ctx.body_bytes).into_owned())),

        ResponsePart::Json => {
            if json_cache.is_none() {
                *json_cache = serde_json::from_slice(&ctx.body_bytes).ok();
            }
            json_cache.as_ref().and_then(|json| {
                let path = assertion.target.as_deref().unwrap_or("$");
                resolve_json_path(json, path).map(value_to_string)
            })
        }

        // Total time spent waiting for the response (total request
        // latency), in milliseconds. This is used to assert against an
        // SLA budget, e.g. "fail if latency exceeds 500ms" using
        // GreaterThan/LessThan/LessThanOrEqual.
        ResponsePart::Latency => Some(ctx.latency_ms.to_string()),

        // Convenience alias for the `content-type` header, so the user
        // does not need to know the exact header name.
        ResponsePart::ContentType => ctx.headers.get("content-type").cloned(),

        // The length of the response body as ACTUALLY received (in bytes),
        // NOT the `content-length` header value (which can be spoofed
        // or missing entirely).
        ResponsePart::ContentLength => Some(ctx.body_bytes.len().to_string()),

        _ => {
            warn!(
                assertion_id = %assertion.id,
                response_part = ?assertion.response_part,
                "Unsupported response part — this assertion will fail"
            );
            None
        }
    };

    // 2. Aiwatar da ma'aikaci.
    let is_json_wildcard = matches!(assertion.response_part, ResponsePart::Json)
        && assertion
            .target
            .as_deref()
            .is_some_and(|t| t.contains("[*]"));

    let (passed, message) = if is_json_wildcard
        && matches!(
            assertion.operator,
            AssertionOperator::Exists | AssertionOperator::Contains
        ) {
        // Wildcard path `$.items[*].field`: we need the list of values
        // (Vec<Value>) so each element is checked individually, instead of
        // comparing the single joined string produced by `actual`.
        let resolved_array: Option<Vec<Value>> = json_cache.as_ref().and_then(|json| {
            let path = assertion.target.as_deref().unwrap_or("$");
            resolve_json_path(json, path).and_then(|v| v.as_array().cloned())
        });

        match &assertion.operator {
            AssertionOperator::Exists => {
                let p = resolved_array.as_ref().is_some_and(|a| !a.is_empty());
                (
                    p,
                    if p {
                        None
                    } else {
                        Some(
                            "The value does not exist, or no element matched the given path".into(),
                        )
                    },
                )
            }

            AssertionOperator::Contains => {
                let expected = assertion.expected_value.as_deref().unwrap_or("");
                let p = resolved_array.as_ref().is_some_and(|arr| {
                    arr.iter().any(|v| {
                        let s = value_to_string(v.clone());
                        s == expected || s.contains(expected)
                    })
                });
                let msg = if p {
                    None
                } else {
                    Some(format!("No element matched {:?}", assertion.expected_value))
                };
                (p, msg)
            }

            _ => unreachable!("the operator was matched above"),
        }
    } else {
        match &assertion.operator {
            AssertionOperator::Exists => {
                let p = actual.is_some();
                (
                    p,
                    if p {
                        None
                    } else {
                        Some("The value does not exist at the given path".into())
                    },
                )
            }

            _ => {
                if let Some(ref actual_val) = actual {
                    let expected = assertion.expected_value.as_deref().unwrap_or("");
                    let p = compare_values(
                        assertion.id,
                        &assertion.operator,
                        actual_val,
                        expected,
                        regex_cache,
                    );
                    let msg = if p {
                        None
                    } else {
                        Some(format!(
                            "Expected {:?} {:?}, but got {:?}",
                            assertion.operator, assertion.expected_value, actual_val
                        ))
                    };
                    (p, msg)
                } else {
                    (false, Some("No value was available to compare".into()))
                }
            }
        }
    };

    AssertionEvaluationResult {
        assertion_id: assertion.id,
        passed,
        expected: assertion.expected_value.clone(),
        actual,
        message,
    }
}

/// Compares the actual value against the expected value using the selected operator.
fn compare_values(
    assertion_id: AssertionId,
    operator: &AssertionOperator,
    actual: &str,
    expected: &str,
    regex_cache: &RegexCache,
) -> bool {
    match operator {
        AssertionOperator::Equals => actual == expected,
        AssertionOperator::NotEquals => actual != expected,
        AssertionOperator::Contains => actual.contains(expected),
        AssertionOperator::NotContains => !actual.contains(expected),

        AssertionOperator::GreaterThan
        | AssertionOperator::GreaterThanOrEqual
        | AssertionOperator::LessThan
        | AssertionOperator::LessThanOrEqual => {
            if let (Ok(a), Ok(b)) = (actual.parse::<f64>(), expected.parse::<f64>()) {
                match operator {
                    AssertionOperator::GreaterThan => a > b,
                    AssertionOperator::GreaterThanOrEqual => a >= b,
                    AssertionOperator::LessThan => a < b,
                    AssertionOperator::LessThanOrEqual => a <= b,
                    _ => unreachable!(),
                }
            } else {
                match operator {
                    AssertionOperator::GreaterThan => actual > expected,
                    AssertionOperator::GreaterThanOrEqual => actual >= expected,
                    AssertionOperator::LessThan => actual < expected,
                    AssertionOperator::LessThanOrEqual => actual <= expected,
                    _ => unreachable!(),
                }
            }
        }

        AssertionOperator::RegexMatch => {
            // Never recompile on the hot path. The cache is keyed
            // by `(assertion_id, pattern)` so an in-place pattern edit is
            // detected and recompiled once, and a stable pattern is reused
            // across every check. We also never log the pattern itself, nor
            // the compiler's error text (which can echo the pattern back).
            match regex_cache.get_or_compile(assertion_id, expected) {
                Some(re) => re.is_match(actual),
                None => {
                    warn!(
                        assertion_id = %assertion_id,
                        error_category = "invalid_regex",
                        "RegexMatch assertion has an invalid pattern at evaluation time"
                    );
                    false
                }
            }
        }

        _ => false,
    }
}

/// A minimal JSON path resolver.
///
/// Beyond basic dot-notation and bracket indexing, it supports:
///   - a `.length` pseudo-property on arrays (element count) and strings
///     (character count), e.g. `$.items.length` or `$.name.length`.
///   - a single-level wildcard, e.g. `$.items[*].id`, which returns an
///     Array holding the `id` of every element in `items`. It is not
///     intended to be a full JSONPath engine.
fn resolve_json_path(root: &Value, path: &str) -> Option<Value> {
    let path = path.trim_start_matches('$').trim_start_matches('.');
    if path.is_empty() {
        return Some(root.clone());
    }

    let segments: Vec<&str> = path.split('.').collect();
    resolve_segments(root, &segments)
}

/// Resolves a list of JSON path segments against a starting value.
/// Split out from `resolve_json_path` so it can be called
/// recursively while resolving `[*]` against each element of an Array.
fn resolve_segments(root: &Value, segments: &[&str]) -> Option<Value> {
    let mut current = root.clone();
    let mut i = 0;

    while i < segments.len() {
        let seg = segments[i];

        if seg == "length" && i == segments.len() - 1 {
            return match &current {
                Value::Array(arr) => Some(Value::from(arr.len())),
                Value::String(s) => Some(Value::from(s.chars().count())),
                Value::Object(map) => Some(Value::from(map.len())),
                _ => None,
            };
        }

        // Yankin `key[*]` ko `key[N]`.
        if let Some(bracket_pos) = seg.find('[') {
            let key = &seg[..bracket_pos];
            let idx_part = &seg[bracket_pos + 1..seg.len() - 1];

            if !key.is_empty() {
                current = current.get(key)?.clone();
            }

            if idx_part == "*" {
                let arr = current.as_array()?;
                let remaining = &segments[i + 1..];
                let mut collected = Vec::with_capacity(arr.len());
                for item in arr {
                    if remaining.is_empty() {
                        collected.push(item.clone());
                    } else {
                        match resolve_segments(item, remaining) {
                            Some(v) => collected.push(v),
                            None => continue,
                        }
                    }
                }
                return Some(Value::Array(collected));
            } else {
                let idx: usize = idx_part.parse().ok()?;
                current = current.get(idx)?.clone();
            }
        } else {
            current = current.get(seg)?.clone();
        }

        i += 1;
    }

    Some(current)
}

fn value_to_string(v: Value) -> String {
    match v {
        Value::String(s) => s,
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::shared::ids::AssertionId;
    use uuid::Uuid;

    fn make_assertion(
        response_part: ResponsePart,
        operator: AssertionOperator,
        target: Option<&str>,
        expected_value: Option<&str>,
    ) -> Assertion {
        Assertion {
            id: AssertionId(Uuid::new_v4()),
            response_part,
            operator,
            target: target.map(String::from),
            expected_value: expected_value.map(String::from),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn make_ctx(status: u16, body: &str) -> EvalContext {
        EvalContext {
            status_code: status,
            headers: HashMap::from([("content-type".into(), "application/json".into())]),
            body_bytes: body.as_bytes().to_vec(),
            latency_ms: 42,
        }
    }

    #[test]
    fn test_status_code_passes() {
        let assertions = vec![make_assertion(
            ResponsePart::StatusCode,
            AssertionOperator::Equals,
            None,
            Some("200"),
        )];
        let ctx = EvalContext {
            status_code: 200,
            ..make_ctx(200, "")
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_status_code_fails() {
        let assertions = vec![make_assertion(
            ResponsePart::StatusCode,
            AssertionOperator::Equals,
            None,
            Some("404"),
        )];
        let ctx = EvalContext {
            status_code: 200,
            ..make_ctx(200, "")
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(!passed);
    }

    #[test]
    fn test_json_equals_passes() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Equals,
            Some("$.name"),
            Some("PingVertex"),
        )];
        let ctx = make_ctx(200, r#"{"name": "PingVertex"}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_exists_passes() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Exists,
            Some("$.user.id"),
            None,
        )];
        let ctx = make_ctx(200, r#"{"user": {"id": 123}}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_header_contains() {
        let assertions = vec![make_assertion(
            ResponsePart::Headers,
            AssertionOperator::Contains,
            Some("content-type"),
            Some("json"),
        )];
        let ctx = EvalContext {
            status_code: 200,
            headers: HashMap::from([("content-type".into(), "application/json".into())]),
            body_bytes: vec![],
            latency_ms: 0,
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_body_contains_text() {
        let assertions = vec![make_assertion(
            ResponsePart::Body,
            AssertionOperator::Contains,
            None,
            Some("Hello"),
        )];
        let ctx = make_ctx(200, "Hello, world!");
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_multiple_assertions_all_pass() {
        let assertions = vec![
            make_assertion(
                ResponsePart::StatusCode,
                AssertionOperator::Equals,
                None,
                Some("200"),
            ),
            make_assertion(
                ResponsePart::Json,
                AssertionOperator::Equals,
                Some("$.status"),
                Some("ok"),
            ),
        ];
        let ctx = make_ctx(200, r#"{"status": "ok"}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_array_length_greater_than() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::GreaterThan,
            Some("$.items.length"),
            Some("0"),
        )];
        let ctx = make_ctx(200, r#"{"items": [1, 2, 3]}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_string_length_equals() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Equals,
            Some("$.name.length"),
            Some("10"),
        )];
        let ctx = make_ctx(200, r#"{"name": "PingVertex"}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_wildcard_contains_matches() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Contains,
            Some("$.items[*].id"),
            Some("42"),
        )];
        let ctx = make_ctx(200, r#"{"items": [{"id": 1}, {"id": 42}, {"id": 7}]}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_wildcard_exists_passes_when_non_empty() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Exists,
            Some("$.items[*].id"),
            None,
        )];
        let ctx = make_ctx(200, r#"{"items": [{"id": 1}, {"id": 2}]}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_json_wildcard_exists_fails_when_empty() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::Exists,
            Some("$.items[*].id"),
            None,
        )];
        let ctx = make_ctx(200, r#"{"items": []}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(!passed);
    }

    #[test]
    fn test_latency_greater_than_fails_when_over_budget() {
        let assertions = vec![make_assertion(
            ResponsePart::Latency,
            AssertionOperator::GreaterThan,
            None,
            Some("500"),
        )];
        let ctx = EvalContext {
            status_code: 200,
            headers: HashMap::new(),
            body_bytes: vec![],
            latency_ms: 750,
        };
        // "fail if latency exceeds 500ms" -> GreaterThan 500 SHOULD pass
        // (i.e. it correctly signals the budget was exceeded).
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);

        let ok_ctx = EvalContext {
            latency_ms: 300,
            ..EvalContext {
                status_code: 200,
                headers: HashMap::new(),
                body_bytes: vec![],
                latency_ms: 0,
            }
        };
        let (passed_within_budget, _) = evaluate_assertions(&assertions, &ok_ctx);
        assert!(!passed_within_budget);
    }

    #[test]
    fn test_latency_less_than_or_equal_passes_within_budget() {
        let assertions = vec![make_assertion(
            ResponsePart::Latency,
            AssertionOperator::LessThanOrEqual,
            None,
            Some("500"),
        )];
        let ctx = EvalContext {
            status_code: 200,
            headers: HashMap::new(),
            body_bytes: vec![],
            latency_ms: 500,
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_headers_exists_passes_for_empty_string_value() {
        let assertions = vec![make_assertion(
            ResponsePart::Headers,
            AssertionOperator::Exists,
            Some("x-content-type-options"),
            None,
        )];
        let ctx = EvalContext {
            status_code: 200,
            headers: HashMap::from([("x-content-type-options".into(), "".into())]),
            body_bytes: vec![],
            latency_ms: 0,
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_headers_exists_security_header_case_insensitive() {
        let assertions = vec![make_assertion(
            ResponsePart::Headers,
            AssertionOperator::Exists,
            Some("Strict-Transport-Security"),
            None,
        )];
        let ctx = EvalContext {
            status_code: 200,
            headers: HashMap::from([(
                "strict-transport-security".into(),
                "max-age=63072000".into(),
            )]),
            body_bytes: vec![],
            latency_ms: 0,
        };
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_content_type_contains_application_json() {
        let assertions = vec![make_assertion(
            ResponsePart::ContentType,
            AssertionOperator::Contains,
            None,
            Some("application/json"),
        )];
        let ctx = make_ctx(200, r#"{"ok": true}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_content_length_less_than_passes_and_fails() {
        let assertions = vec![make_assertion(
            ResponsePart::ContentLength,
            AssertionOperator::LessThan,
            None,
            Some("100000"),
        )];
        let small_ctx = make_ctx(200, "short body");
        let (passed, _) = evaluate_assertions(&assertions, &small_ctx);
        assert!(passed);

        let big_body = "x".repeat(100_001);
        let big_ctx = make_ctx(200, &big_body);
        let (passed_big, _) = evaluate_assertions(&assertions, &big_ctx);
        assert!(!passed_big);
    }

    #[test]
    fn test_one_fails_all_fail() {
        let assertions = vec![
            make_assertion(
                ResponsePart::StatusCode,
                AssertionOperator::Equals,
                None,
                Some("200"),
            ),
            make_assertion(
                ResponsePart::Json,
                AssertionOperator::Equals,
                Some("$.status"),
                Some("error"),
            ),
        ];
        let ctx = make_ctx(200, r#"{"status": "ok"}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(!passed);
    }

    #[test]
    fn test_regex_match_passes_on_body() {
        let assertions = vec![make_assertion(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(r"^Hello"),
        )];
        let ctx = make_ctx(200, "Hello, world!");
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);
    }

    #[test]
    fn test_regex_match_fails_when_pattern_does_not_match() {
        let assertions = vec![make_assertion(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(r"^Goodbye"),
        )];
        let ctx = make_ctx(200, "Hello, world!");
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(!passed);
    }

    #[test]
    fn test_regex_match_on_json_field() {
        let assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::RegexMatch,
            Some("$.email"),
            Some(r"^[\w.+-]+@[\w-]+\.[a-zA-Z]{2,}$"),
        )];
        let ctx = make_ctx(200, r#"{"email": "user@example.com"}"#);
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(passed);

        let bad_assertions = vec![make_assertion(
            ResponsePart::Json,
            AssertionOperator::RegexMatch,
            Some("$.email"),
            Some(r"^[\w.+-]+@[\w-]+\.[a-zA-Z]{2,}$"),
        )];
        let bad_ctx = make_ctx(200, r#"{"email": "not-an-email"}"#);
        let (bad_passed, _) = evaluate_assertions(&bad_assertions, &bad_ctx);
        assert!(!bad_passed);
    }

    #[test]
    fn test_regex_match_invalid_pattern_returns_false_without_panicking() {
        let assertions = vec![make_assertion(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            // Bracket da ba a rufe ba — invalid regex syntax.
            Some(r"[unclosed"),
        )];
        let ctx = make_ctx(200, "Hello, world!");
        // Must not panic; it must simply fail (return false).
        let (passed, _) = evaluate_assertions(&assertions, &ctx);
        assert!(!passed);
    }
}

#[cfg(test)]
mod part10_tests {
    use super::*;
    use crate::regex_cache::RegexCache;
    use domain::enums::{AssertionOperator, ResponsePart};
    use domain::shared::ids::AssertionId;

    fn assertion_with_id(
        id: AssertionId,
        operator: AssertionOperator,
        expected: &str,
    ) -> Assertion {
        Assertion {
            id,
            response_part: ResponsePart::Body,
            operator,
            target: None,
            expected_value: Some(expected.to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn ctx() -> EvalContext {
        EvalContext {
            status_code: 200,
            headers: HashMap::new(),
            body_bytes: b"Hello, world!".to_vec(),
            latency_ms: 1,
        }
    }

    #[test]
    fn repeated_evaluation_does_not_recompile_the_same_pattern() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();
        let assertions = vec![assertion_with_id(
            id,
            AssertionOperator::RegexMatch,
            r"^Hello",
        )];

        // Evaluate three times with the same cache. Only the first call
        // should hit `Regex::new`.
        for _ in 0..3 {
            let (passed, _) = evaluate_assertions_with_cache(&assertions, &ctx(), &cache);
            assert!(passed);
        }

        assert_eq!(
            cache.compile_count(),
            1,
            "repeated evaluation must compile the regex exactly once"
        );
    }

    #[test]
    fn changing_the_pattern_forcibly_recompiles() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();

        let a = vec![assertion_with_id(
            id,
            AssertionOperator::RegexMatch,
            r"^Hello",
        )];
        let (passed, _) = evaluate_assertions_with_cache(&a, &ctx(), &cache);
        assert!(passed);
        assert_eq!(cache.compile_count(), 1);

        // Same ID, different pattern — the cache must invalidate.
        let b = vec![assertion_with_id(
            id,
            AssertionOperator::RegexMatch,
            r"^Goodbye",
        )];
        let (passed, _) = evaluate_assertions_with_cache(&b, &ctx(), &cache);
        assert!(!passed, "the new pattern should not match the body");
        assert_eq!(
            cache.compile_count(),
            2,
            "editing the pattern must force exactly one recompile"
        );
    }

    #[test]
    fn invalid_pattern_at_evaluation_time_fails_closed() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();
        // This should have been rejected at write time, but we exercise
        // the defensive path in case of legacy data.
        let assertions = vec![assertion_with_id(
            id,
            AssertionOperator::RegexMatch,
            "[unclosed",
        )];

        let (passed, results) = evaluate_assertions_with_cache(&assertions, &ctx(), &cache);
        assert!(!passed, "an invalid regex must fail the assertion");
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        // The cache must not have stored a bad entry.
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cached_regex_arc_is_reused_across_evaluations() {
        // A direct, non-global check that the same compiled regex is
        // handed out on the second evaluation.
        let cache = RegexCache::new(16);
        let id = AssertionId::new();
        let pattern = r"^Hello";

        let first = cache.get_or_compile(id, pattern).unwrap();
        let second = cache.get_or_compile(id, pattern).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }
}

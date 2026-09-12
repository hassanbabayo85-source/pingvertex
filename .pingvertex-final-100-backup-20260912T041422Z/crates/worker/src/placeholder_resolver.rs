use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

/// Sakamakon da aka samu daga gudanar da step guda a cikin API flow — ana
/// amfani da shi domin warware placeholders (`{{steps.N.body.path}}` da
/// sauransu) na steps na gaba.
pub struct StepResult {
    pub status_code: u16,
    /// Kowane maɓalli an maida shi ƙaramin harafi (lowercase), bin salon
    /// `EvalContext` a `assertion_evaluator.rs`.
    pub headers: HashMap<String, String>,
    pub body: String,
}

fn placeholder_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\{\{steps\.(\d+)\.(body|header|status)(?:\.([a-zA-Z0-9_.-]+))?\}\}")
            .expect("placeholder regex is a valid, statically-known pattern")
    })
}

/// Percent-encodes an extracted value before it is spliced into a template,
/// using `url::form_urlencoded`'s byte serializer. This neutralizes `&`,
/// `?`, `#`, `/`, `%`, and other characters that could otherwise let an
/// extracted value change the semantics of a `url_template` (e.g. injecting
/// a new query parameter) or a header value.
fn percent_encode_value(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// Traverses a simple dot-separated JSON path (e.g. `"data.access_token"`)
/// starting at `root`. Only plain object-key traversal is supported — no
/// array indices or wildcards, matching the restricted placeholder grammar.
fn resolve_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for key in path.split('.') {
        current = current.as_object()?.get(key)?;
    }
    Some(current)
}

fn json_value_to_placeholder_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(format!(
            "value at json path is not a string or number: {other:?}"
        )),
    }
}

/// Replaces `{{steps.N.body|header|status[.path]}}` placeholders in
/// `template` with the corresponding values from `results`, sourced from
/// steps that already ran (`N < current_step`).
///
/// This is a pure function: no I/O, no HTTP calls. A single left-to-right
/// pass is made over the *original* `template` — the output of a
/// substitution is never re-scanned for further placeholders, so a
/// maliciously nested placeholder (e.g. an extracted value that itself
/// looks like `{{steps.N.body}}`) is never re-resolved.
pub fn resolve_template(
    template: &str,
    results: &HashMap<u16, StepResult>,
    current_step: u16,
) -> Result<String, String> {
    let re = placeholder_regex();

    let mut output = String::with_capacity(template.len());
    let mut last_end = 0;

    for caps in re.captures_iter(template) {
        let whole_match = caps.get(0).expect("group 0 always present on a match");
        output.push_str(&template[last_end..whole_match.start()]);
        last_end = whole_match.end();

        let step_num: u16 = caps[1]
            .parse()
            .map_err(|_| format!("invalid step number in placeholder: {}", &caps[1]))?;

        if step_num >= current_step {
            return Err(format!("step {step_num} is not yet available"));
        }

        let step_result = results
            .get(&step_num)
            .ok_or_else(|| format!("step {step_num} result not found"))?;

        let kind = &caps[2];
        let path = caps.get(3).map(|m| m.as_str());

        let raw_value: String = match kind {
            "status" => step_result.status_code.to_string(),

            "header" => {
                let header_name =
                    path.ok_or_else(|| "header placeholder is missing a header name".to_string())?;
                step_result
                    .headers
                    .get(&header_name.to_lowercase())
                    .cloned()
                    .ok_or_else(|| {
                        format!("header '{header_name}' not found in step {step_num} response")
                    })?
            }

            "body" => {
                let json: Value = serde_json::from_str(&step_result.body)
                    .map_err(|_| format!("step {step_num} body is not valid JSON"))?;

                let target = match path {
                    Some(p) => resolve_json_path(&json, p).ok_or_else(|| {
                        format!("json path '{p}' not found in step {step_num} body")
                    })?,
                    None => &json,
                };

                json_value_to_placeholder_string(target)?
            }

            other => unreachable!("regex only matches body|header|status, got {other}"),
        };

        // CRLF / header-injection guard — applied unconditionally, since
        // this function does not know whether the caller will splice the
        // result into a URL or a header value.
        if raw_value.contains('\r') || raw_value.contains('\n') {
            return Err(format!(
                "extracted value for step {step_num} contains CR/LF characters"
            ));
        }

        output.push_str(&percent_encode_value(&raw_value));
    }

    output.push_str(&template[last_end..]);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(status_code: u16, headers: &[(&str, &str)], body: &str) -> StepResult {
        StepResult {
            status_code,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                .collect(),
            body: body.to_string(),
        }
    }

    #[test]
    fn resolves_body_placeholder() {
        let mut results = HashMap::new();
        results.insert(1, step(200, &[], r#"{"token": "abc123"}"#));

        let out = resolve_template("Bearer {{steps.1.body.token}}", &results, 2).unwrap();

        assert_eq!(out, "Bearer abc123");
    }

    #[test]
    fn resolves_header_placeholder() {
        let mut results = HashMap::new();
        results.insert(1, step(200, &[("X-Request-Id", "req-42")], "{}"));

        let out = resolve_template("{{steps.1.header.x-request-id}}", &results, 2).unwrap();

        assert_eq!(out, "req-42");
    }

    #[test]
    fn resolves_status_placeholder() {
        let mut results = HashMap::new();
        results.insert(1, step(201, &[], "{}"));

        let out = resolve_template("status={{steps.1.status}}", &results, 2).unwrap();

        assert_eq!(out, "status=201");
    }

    #[test]
    fn resolves_nested_json_path() {
        let mut results = HashMap::new();
        results.insert(1, step(200, &[], r#"{"data": {"access_token": "xyz789"}}"#));

        let out = resolve_template("{{steps.1.body.data.access_token}}", &results, 2).unwrap();

        assert_eq!(out, "xyz789");
    }

    #[test]
    fn rejects_step_that_does_not_exist() {
        let results: HashMap<u16, StepResult> = HashMap::new();

        let err = resolve_template("{{steps.1.body.token}}", &results, 2).unwrap_err();

        assert!(err.contains("not found"));
    }

    #[test]
    fn rejects_step_number_greater_or_equal_to_current_step() {
        let mut results = HashMap::new();
        results.insert(2, step(200, &[], r#"{"token": "abc"}"#));

        // Step 2 referencing itself.
        let err_self = resolve_template("{{steps.2.body.token}}", &results, 2).unwrap_err();
        assert!(err_self.contains("not yet available"));

        // Step 2 referencing a future step (3).
        let err_future = resolve_template("{{steps.3.body.token}}", &results, 2).unwrap_err();
        assert!(err_future.contains("not yet available"));
    }

    #[test]
    fn leaves_malformed_placeholder_syntax_untouched() {
        let results: HashMap<u16, StepResult> = HashMap::new();

        // Missing `{{ }}` braces entirely — not a placeholder at all.
        let out = resolve_template("steps.1.body", &results, 2).unwrap();

        assert_eq!(out, "steps.1.body");
    }

    #[test]
    fn injection_attempt_percent_encodes_ampersand_in_url_query() {
        let mut results = HashMap::new();
        results.insert(1, step(200, &[], r#"{"token": "abc&admin=true"}"#));

        let out = resolve_template(
            "https://example.com/api?token={{steps.1.body.token}}",
            &results,
            2,
        )
        .unwrap();

        assert!(!out.contains('&'));
        assert!(!out.contains("admin=true"));
        assert!(out.contains("%26admin%3Dtrue"));
    }

    #[test]
    fn injection_attempt_rejects_crlf_header_injection() {
        let mut results = HashMap::new();
        results.insert(1, step(200, &[], r#"{"token": "abc\r\nX-Injected: evil"}"#));

        let err = resolve_template("Bearer {{steps.1.body.token}}", &results, 2).unwrap_err();

        assert!(err.contains("CR/LF"));
    }

    #[test]
    fn injection_attempt_nested_placeholder_is_not_re_scanned() {
        let mut results = HashMap::new();
        // Step 1's body resolves to the digit "9" — if the implementation
        // incorrectly re-scanned its own output, it would then try (and
        // fail) to resolve a bogus `{{steps.9.body}}` placeholder.
        results.insert(1, step(200, &[], r#""9""#));

        let out = resolve_template("{{steps.{{steps.1.body}}.body}}", &results, 2).unwrap();

        // Only the inner, well-formed placeholder was matched and replaced
        // in a single left-to-right pass over the original template; the
        // surrounding `{{steps.` / `.body}}` text was never re-parsed.
        assert_eq!(out, "{{steps.9.body}}");
    }
}

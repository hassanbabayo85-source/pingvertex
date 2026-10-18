//! Application-layer header DTO.
//!
//! SECURITY: `value` is plaintext (or a template). If `secret == true`
//! the value will be encrypted at rest before persistence; the `Debug`
//! impl below redacts it so a `tracing::info!(?request)` on the DTO
//! cannot leak the plaintext into logs.

/// A header as received from the HTTP layer, on its way to become a
/// domain `FlowHeader`.
#[derive(Clone)]
pub struct FlowHeaderInput {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

impl std::fmt::Debug for FlowHeaderInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered_value: &str = if self.secret {
            "***REDACTED***"
        } else {
            self.value.as_str()
        };
        f.debug_struct("FlowHeaderInput")
            .field("name", &self.name)
            .field("value", &rendered_value)
            .field("secret", &self.secret)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secret_value() {
        let h = FlowHeaderInput {
            name: "Authorization".into(),
            value: "Bearer SUPER_SECRET".into(),
            secret: true,
        };
        let rendered = format!("{h:?}");
        assert!(!rendered.contains("SUPER_SECRET"));
        assert!(rendered.contains("***REDACTED***"));
    }

    #[test]
    fn debug_shows_plain_value() {
        let h = FlowHeaderInput {
            name: "Content-Type".into(),
            value: "application/json".into(),
            secret: false,
        };
        let rendered = format!("{h:?}");
        assert!(rendered.contains("application/json"));
    }
}

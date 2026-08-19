use crate::errors::DomainError;

/// A validated E.164 phone number (e.g. `+2348012345678`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        // Normalize: trim, remove spaces/dashes/parentheses for lenient input,
        // but enforce E.164 output format.
        let normalized: String = value
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '(' && *c != ')')
            .collect();

        if normalized.is_empty() {
            return Err(DomainError::InvalidPhoneNumber);
        }

        if !normalized.starts_with('+') {
            return Err(DomainError::InvalidPhoneNumber);
        }

        let digits = &normalized[1..];
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::InvalidPhoneNumber);
        }

        // E.164: country code 1-3 digits, total max 15 digits.
        if digits.len() < 8 || digits.len() > 15 {
            return Err(DomainError::InvalidPhoneNumber);
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PhoneNumber {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_e164() {
        assert!(PhoneNumber::new("+2348012345678").is_ok());
    }

    #[test]
    fn normalizes_and_accepts_formatted_number() {
        let phone = PhoneNumber::new(" +1 (555) 123-4567 ").unwrap();
        assert_eq!(phone.as_str(), "+15551234567");
    }

    #[test]
    fn rejects_missing_plus() {
        assert_eq!(
            PhoneNumber::new("2348012345678"),
            Err(DomainError::InvalidPhoneNumber)
        );
    }

    #[test]
    fn rejects_too_short() {
        assert_eq!(
            PhoneNumber::new("+123456"),
            Err(DomainError::InvalidPhoneNumber)
        );
    }
}

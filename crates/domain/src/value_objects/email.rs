use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    Empty,
    TooLong,
    InvalidFormat,
}

impl fmt::Display for EmailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmailError::Empty => write!(f, "email cannot be empty"),
            EmailError::TooLong => write!(f, "email is too long"),
            EmailError::InvalidFormat => write!(f, "invalid email format"),
        }
    }
}

impl std::error::Error for EmailError {}

impl Email {
    pub fn new(value: impl Into<String>) -> Result<Self, EmailError> {
        let email = value.into().trim().to_ascii_lowercase();

        if email.is_empty() {
            return Err(EmailError::Empty);
        }

        if email.len() > 254 {
            return Err(EmailError::TooLong);
        }

        let parts: Vec<&str> = email.split('@').collect();

        if parts.len() != 2 {
            return Err(EmailError::InvalidFormat);
        }

        if parts[0].is_empty() || parts[1].is_empty() {
            return Err(EmailError::InvalidFormat);
        }

        if !parts[1].contains('.') {
            return Err(EmailError::InvalidFormat);
        }

        Ok(Self(email))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

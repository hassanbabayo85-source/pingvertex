use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsernameError {
    Empty,
    TooShort,
    TooLong,
    InvalidCharacter,
}

impl fmt::Display for UsernameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsernameError::Empty => write!(f, "username cannot be empty"),
            UsernameError::TooShort => write!(f, "username is too short"),
            UsernameError::TooLong => write!(f, "username is too long"),
            UsernameError::InvalidCharacter => {
                write!(f, "username contains invalid characters")
            }
        }
    }
}

impl std::error::Error for UsernameError {}

impl Username {
    pub fn new(value: impl Into<String>) -> Result<Self, UsernameError> {
        let username = value.into().trim().to_string();

        if username.is_empty() {
            return Err(UsernameError::Empty);
        }

        if username.len() < 3 {
            return Err(UsernameError::TooShort);
        }

        if username.len() > 32 {
            return Err(UsernameError::TooLong);
        }

        if !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(UsernameError::InvalidCharacter);
        }

        Ok(Self(username))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

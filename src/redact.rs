use serde::{Deserialize, Serialize};
use std::fmt;

/// Wrapper that redacts sensitive values in Display and Debug output.
///
/// Shows only the first N characters followed by "...".
/// Use for API keys, passwords, tokens, and other secrets.
///
/// # Example
/// ```
/// let key = deko::redact::Redacted::new("sk-abc123def456");
/// assert_eq!(format!("{}", key), "sk-abc...");
/// ```
#[derive(Clone, Serialize, Deserialize)]
pub struct Redacted(String);

impl Redacted {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let visible: String = self.0.chars().take(6).collect();
        if self.0.len() <= 6 {
            write!(f, "***")
        } else {
            write!(f, "{}...", visible)
        }
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

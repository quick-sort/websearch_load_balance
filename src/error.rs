//! Error types for web search load balancing.

use thiserror::Error;

/// Result type alias using our error type.
pub type Result<T> = std::result::Result<T, WebSearchError>;

/// Main error type for web search operations.
#[derive(Debug, Error)]
pub enum WebSearchError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Network request error.
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Provider API returned an error.
    #[error("Provider API error: code {0}, message: {1}")]
    ProviderError(i32, String),

    /// All providers failed.
    #[error("All providers failed: {0}")]
    AllProvidersFailed(String),

    /// Authentication error (invalid API key).
    #[error("Authentication error: {0}")]
    AuthError(String),

    /// Operation not supported by provider.
    #[error("Operation not supported: {0}")]
    NotSupported(String),

    /// JSON parsing error.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// No enabled providers available.
    #[error("No available providers")]
    NoProvidersAvailable,

    /// No API keys configured for provider.
    #[error("No API keys for provider: {0}")]
    NoApiKeys(String),
}

impl WebSearchError {
    /// Check if error is an auth error (invalid API key).
    pub fn is_auth_error(&self) -> bool {
        matches!(self, WebSearchError::AuthError(_) | WebSearchError::ProviderError(1004, _))
    }

    /// Check if error is a transient error (network, rate limit).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            WebSearchError::NetworkError(_)
                | WebSearchError::ProviderError(1302, _) // rate limit
                | WebSearchError::ProviderError(1305, _) // overload
        )
    }
}

/// Parse API key from environment variable.
///
/// Format: `API_KEYS` - JSON array `["key1","key2"]` - returns first key
pub fn parse_api_key(name: &str) -> String {
    // Try KEYS format (JSON array)
    let plural = format!("{}_KEYS", name);
    if let Ok(v) = std::env::var(&plural) {
        if !v.is_empty() && v != "[]" {
            if let Ok(keys) = serde_json::from_str::<Vec<String>>(&v) {
                if let Some(first) = keys.into_iter().next() {
                    if !first.is_empty() {
                        return first;
                    }
                }
            }
        }
    }

    String::new()
}
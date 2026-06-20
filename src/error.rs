//! Error and result types for the crate.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Anything that can go wrong talking to the Notifications API.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Transport-level failure (DNS, TLS, timeout, connection reset, ...).
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// The API returned a non-success status with an error body.
    #[error("GitHub API error {status}: {message}")]
    Api {
        /// HTTP status code returned by the API.
        status: StatusCode,
        /// Human-readable message from the response body, if any.
        message: String,
        /// Link to the relevant API documentation, if the body provided one.
        doc_url: Option<String>,
    },

    /// A rate limit was hit. `kind` distinguishes the primary hourly budget from
    /// secondary (abuse) limits, which use `Retry-After`.
    #[error("rate limited ({kind:?}); retry_after={retry_after:?} reset_at={reset_at:?}")]
    RateLimited {
        /// Which rate limit was hit.
        kind: RateLimitKind,
        /// Server-suggested wait, from `Retry-After` (secondary limits).
        retry_after: Option<Duration>,
        /// When the primary budget resets, from `x-ratelimit-reset`.
        reset_at: Option<DateTime<Utc>>,
    },

    /// Authentication failed or the token is missing required scopes.
    #[error("authentication failed or token lacks required scopes")]
    Unauthorized,

    /// The response body did not match the expected shape. The raw body is kept
    /// so callers can see exactly what GitHub sent.
    #[error("failed to deserialize response body: {source}")]
    Deserialize {
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
        /// The raw response body that failed to parse.
        body: String,
    },

    /// The configured base URL could not be parsed/joined.
    #[error("invalid base URL")]
    InvalidBaseUrl,

    /// A [`TokenProvider`](crate::TokenProvider) failed to produce a token.
    #[error("token provider failed: {0}")]
    Token(String),

    /// Filesystem I/O failed, e.g. in a file-backed state store.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Distinguishes GitHub's two rate-limit regimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitKind {
    /// The primary hourly request budget (`x-ratelimit-*`).
    Primary,
    /// Secondary / abuse-detection limits, signalled via `Retry-After`.
    Secondary,
}

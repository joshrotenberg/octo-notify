//! Rate-limit accounting parsed from response headers.

use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::HeaderMap;

/// A snapshot of the GitHub rate-limit headers from one response.
///
/// All fields are optional because the headers are absent on some responses
/// (notably `304 Not Modified`, which costs no budget at all).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimit {
    /// Maximum requests permitted in the current window (`x-ratelimit-limit`).
    pub limit: Option<u32>,
    /// Requests remaining in the current window (`x-ratelimit-remaining`).
    pub remaining: Option<u32>,
    /// Requests used in the current window (`x-ratelimit-used`).
    pub used: Option<u32>,
    /// When the current window resets (`x-ratelimit-reset`, a Unix timestamp).
    pub reset_at: Option<DateTime<Utc>>,
    /// Which rate-limit resource this applies to (`x-ratelimit-resource`).
    pub resource: Option<String>,
}

impl RateLimit {
    pub(crate) fn from_headers(headers: &HeaderMap) -> Self {
        let num = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok())
        };
        let reset_at = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(|secs| Utc.timestamp_opt(secs, 0).single());

        RateLimit {
            limit: num("x-ratelimit-limit"),
            remaining: num("x-ratelimit-remaining"),
            used: num("x-ratelimit-used"),
            reset_at,
            resource: headers
                .get("x-ratelimit-resource")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        }
    }
}

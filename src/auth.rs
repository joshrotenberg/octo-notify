//! Authentication.
//!
//! The notifications inbox is inherently *user-scoped*, so authentication is a single
//! bearer token. A classic PAT needs the `notifications` scope (or `repo` to also read
//! issue/commit subjects); a fine-grained PAT needs the read-only "Notifications" account
//! permission; a GitHub App user-to-server token works too.
//!
//! Tokens are supplied through a [`TokenProvider`]. The common case is a static token
//! ([`Auth::token`]), but the trait seam lets a long-running poller refresh expiring
//! user-to-server tokens without any breaking change to the API.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

use crate::error::{Error, Result};

#[cfg(feature = "token-refresh")]
use std::future::Future;
#[cfg(feature = "token-refresh")]
use std::pin::Pin;
#[cfg(feature = "token-refresh")]
use std::time::Duration;

#[cfg(feature = "token-refresh")]
use chrono::{DateTime, Utc};

/// Supplies the bearer token used for each request.
///
/// Implement this for credentials that change over time (e.g. GitHub App
/// user-to-server tokens that expire). The client calls [`token`](TokenProvider::token)
/// per request, so an implementation is free to refresh and cache internally.
#[async_trait]
pub trait TokenProvider: Send + Sync + fmt::Debug {
    /// Produce the current bearer token.
    async fn token(&self) -> Result<SecretString>;
}

/// How the client authenticates.
///
/// Cheap to clone (an `Arc` internally).
#[derive(Clone)]
pub struct Auth {
    provider: Arc<dyn TokenProvider>,
}

impl Auth {
    /// Authenticate with a fixed token (classic/fine-grained PAT or OAuth token).
    pub fn token(token: impl Into<String>) -> Self {
        Auth {
            provider: Arc::new(StaticToken::new(token.into())),
        }
    }

    /// Authenticate with a custom [`TokenProvider`], for refreshable credentials.
    pub fn provider(provider: impl TokenProvider + 'static) -> Self {
        Auth {
            provider: Arc::new(provider),
        }
    }

    /// Read a token from `GITHUB_TOKEN` or `GH_TOKEN`.
    pub fn from_env() -> Result<Self> {
        for key in ["GITHUB_TOKEN", "GH_TOKEN"] {
            if let Ok(value) = std::env::var(key) {
                if !value.is_empty() {
                    return Ok(Self::token(value));
                }
            }
        }
        Err(Error::Token(
            "no GITHUB_TOKEN or GH_TOKEN found in environment".to_owned(),
        ))
    }

    /// Resolve the current bearer token. Used internally per request.
    pub(crate) async fn bearer(&self) -> Result<SecretString> {
        self.provider.token().await
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Auth")
            .field("provider", &self.provider)
            .finish()
    }
}

/// A fixed token. Its `Debug` redacts the secret.
struct StaticToken {
    token: SecretString,
}

impl StaticToken {
    fn new(raw: String) -> Self {
        StaticToken {
            token: SecretString::new(raw.into_boxed_str()),
        }
    }
}

impl fmt::Debug for StaticToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StaticToken(REDACTED)")
    }
}

#[async_trait]
impl TokenProvider for StaticToken {
    async fn token(&self) -> Result<SecretString> {
        // Hand out a fresh wrapper so the stored secret is never moved or cloned directly.
        Ok(SecretString::new(
            self.token.expose_secret().to_owned().into_boxed_str(),
        ))
    }
}

#[cfg(feature = "token-refresh")]
type RefreshFuture = Pin<Box<dyn Future<Output = Result<(SecretString, DateTime<Utc>)>> + Send>>;

#[cfg(feature = "token-refresh")]
type RefreshFn = Arc<dyn Fn() -> RefreshFuture + Send + Sync>;

/// A [`TokenProvider`] that caches a token and refreshes it via a caller-supplied async
/// function before it expires.
///
/// The token is reused until it is within the refresh lead time (default 60 seconds) of its
/// expiry. Concurrent callers on a stale or empty cache share a single in-flight refresh. This
/// crate does not implement any GitHub App auth flow; the caller supplies the async function
/// that mints a token and returns it with its expiry. Requires the `token-refresh` feature.
///
/// ```no_run
/// use std::time::Duration;
/// use octo_notify::{Auth, RefreshingToken, SecretString};
/// use chrono::{Utc, Duration as ChronoDuration};
///
/// let auth = Auth::provider(
///     RefreshingToken::new(|| async {
///         // mint a user-to-server token however you like
///         Ok((SecretString::new("token".to_owned().into_boxed_str()), Utc::now() + ChronoDuration::hours(1)))
///     })
///     .refresh_before(Duration::from_secs(120)),
/// );
/// ```
#[cfg(feature = "token-refresh")]
pub struct RefreshingToken {
    refresh: RefreshFn,
    refresh_before: Duration,
    cache: tokio::sync::Mutex<Option<Cached>>,
}

#[cfg(feature = "token-refresh")]
struct Cached {
    token: SecretString,
    expires_at: DateTime<Utc>,
}

#[cfg(feature = "token-refresh")]
impl RefreshingToken {
    /// Create a provider that obtains tokens via `refresh`, which returns a token and its
    /// expiry.
    pub fn new<F, Fut>(refresh: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(SecretString, DateTime<Utc>)>> + Send + 'static,
    {
        RefreshingToken {
            refresh: Arc::new(move || Box::pin(refresh())),
            refresh_before: Duration::from_secs(60),
            cache: tokio::sync::Mutex::new(None),
        }
    }

    /// Refresh this long before the token's expiry (default 60 seconds).
    pub fn refresh_before(mut self, lead: Duration) -> Self {
        self.refresh_before = lead;
        self
    }

    fn is_fresh(&self, cached: &Cached) -> bool {
        let lead = chrono::Duration::from_std(self.refresh_before)
            .unwrap_or_else(|_| chrono::Duration::zero());
        Utc::now() + lead < cached.expires_at
    }
}

#[cfg(feature = "token-refresh")]
impl fmt::Debug for RefreshingToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshingToken")
            .field("refresh_before", &self.refresh_before)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "token-refresh")]
#[async_trait]
impl TokenProvider for RefreshingToken {
    async fn token(&self) -> Result<SecretString> {
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            if self.is_fresh(cached) {
                return Ok(clone_secret(&cached.token));
            }
        }
        // Hold the async lock across the refresh so concurrent callers share one in-flight call.
        let (token, expires_at) = (self.refresh)().await?;
        let handed_out = clone_secret(&token);
        *cache = Some(Cached { token, expires_at });
        Ok(handed_out)
    }
}

#[cfg(feature = "token-refresh")]
fn clone_secret(secret: &SecretString) -> SecretString {
    SecretString::new(secret.expose_secret().to_owned().into_boxed_str())
}

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

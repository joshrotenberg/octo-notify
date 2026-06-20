//! The HTTP client: configuration, request execution, and response interpretation.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::auth::Auth;
use crate::endpoints::notifications::NotificationsHandler;
use crate::error::{Error, RateLimitKind, Result};
use crate::pagination::{Listing, NotModified, Page, parse_link_next};
use crate::rate_limit::RateLimit;

const DEFAULT_BASE_URL: &str = "https://api.github.com/";
const DEFAULT_USER_AGENT: &str = concat!("octo-notify/", env!("CARGO_PKG_VERSION"));
const GITHUB_API_VERSION: &str = "2022-11-28";

/// A client for the GitHub Notifications API.
///
/// Cheap to clone; clones share the underlying connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
    auth: Auth,
}

impl Client {
    /// Start building a client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Create a client with default settings and the given authentication.
    pub fn new(auth: Auth) -> Result<Self> {
        ClientBuilder::new().auth(auth).build()
    }

    /// Operations on the authenticated user's whole notification inbox.
    pub fn notifications(&self) -> NotificationsHandler<'_> {
        NotificationsHandler { client: self }
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn auth(&self) -> &Auth {
        &self.auth
    }

    /// Join a relative API path onto the configured base URL.
    pub(crate) fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url.join(path).map_err(|_| Error::InvalidBaseUrl)
    }

    /// Interpret a listing response into a [`Listing`], mapping status codes to
    /// the right success/error shapes (notably treating `304` as success).
    pub(crate) async fn interpret_list<T>(&self, resp: reqwest::Response) -> Result<Listing<T>>
    where
        T: DeserializeOwned,
    {
        let status = resp.status();
        let rate_limit = RateLimit::from_headers(resp.headers());
        let poll_interval = parse_poll_interval(resp.headers());
        let last_modified = header_string(resp.headers(), reqwest::header::LAST_MODIFIED);

        match status {
            StatusCode::OK => {
                let next = parse_link_next(resp.headers());
                let body = resp.text().await?;
                let items = serde_json::from_str::<Vec<T>>(&body)
                    .map_err(|source| Error::Deserialize { source, body })?;
                Ok(Listing::Modified(Page {
                    items,
                    poll_interval,
                    last_modified,
                    rate_limit,
                    next,
                }))
            }
            StatusCode::NOT_MODIFIED => Ok(Listing::NotModified(NotModified {
                poll_interval,
                last_modified,
                rate_limit,
            })),
            StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = parse_retry_after(resp.headers());
                if retry_after.is_some() {
                    Err(Error::RateLimited {
                        kind: RateLimitKind::Secondary,
                        retry_after,
                        reset_at: rate_limit.reset_at,
                    })
                } else if rate_limit.remaining == Some(0) {
                    Err(Error::RateLimited {
                        kind: RateLimitKind::Primary,
                        retry_after: None,
                        reset_at: rate_limit.reset_at,
                    })
                } else {
                    Err(api_error(status, resp).await)
                }
            }
            _ => Err(api_error(status, resp).await),
        }
    }
}

/// Builder for [`Client`].
#[derive(Debug)]
pub struct ClientBuilder {
    auth: Option<Auth>,
    base_url: String,
    user_agent: String,
    http: Option<reqwest::Client>,
}

impl ClientBuilder {
    fn new() -> Self {
        ClientBuilder {
            auth: None,
            base_url: DEFAULT_BASE_URL.to_owned(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            http: None,
        }
    }

    /// Set the authentication (required).
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Override the `User-Agent` header (GitHub requires one; a default is set).
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Override the API base URL. Defaults to `https://api.github.com`.
    /// Use the GHES form, e.g. `https://ghe.example.com/api/v3`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Provide a preconfigured `reqwest::Client` (for proxies, timeouts, etc.).
    /// Default headers managed by this crate are still applied on top.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Build the [`Client`].
    pub fn build(self) -> Result<Client> {
        let auth = self
            .auth
            .ok_or_else(|| Error::Token("no authentication configured".to_owned()))?;

        // Normalize the base URL so relative joins keep the full path (matters for GHES).
        let mut base = self.base_url;
        if !base.ends_with('/') {
            base.push('/');
        }
        let base_url = Url::parse(&base).map_err(|_| Error::InvalidBaseUrl)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            HeaderName::from_static("x-github-api-version"),
            HeaderValue::from_static(GITHUB_API_VERSION),
        );
        let ua = HeaderValue::from_str(&self.user_agent)
            .map_err(|e| Error::Token(format!("invalid user agent: {e}")))?;
        headers.insert(USER_AGENT, ua);

        let http = match self.http {
            Some(client) => client,
            None => reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .map_err(Error::Http)?,
        };

        Ok(Client {
            http,
            base_url,
            auth,
        })
    }
}

#[derive(Deserialize)]
struct ApiErrorBody {
    message: Option<String>,
    documentation_url: Option<String>,
}

/// Build an [`Error::Api`] from a failed response, extracting the message/doc URL if present.
async fn api_error(status: StatusCode, resp: reqwest::Response) -> Error {
    let body = resp.text().await.unwrap_or_default();
    let parsed = serde_json::from_str::<ApiErrorBody>(&body).ok();
    let message = parsed
        .as_ref()
        .and_then(|b| b.message.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            if body.is_empty() {
                status
                    .canonical_reason()
                    .unwrap_or("request failed")
                    .to_owned()
            } else {
                body
            }
        });
    let doc_url = parsed.and_then(|b| b.documentation_url);
    Error::Api {
        status,
        message,
        doc_url,
    }
}

fn parse_poll_interval(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get("x-poll-interval")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn header_string(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

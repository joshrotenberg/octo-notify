//! The HTTP client: configuration, request execution, and response interpretation.

use std::time::Duration;

use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use reqwest::{Method, StatusCode};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::auth::Auth;
use crate::endpoints::notifications::NotificationsHandler;
use crate::endpoints::repo::RepoHandler;
use crate::endpoints::threads::ThreadHandler;
use crate::error::{Error, RateLimitKind, Result};
use crate::models::ThreadId;
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

    /// Operations scoped to a single repository's notifications.
    pub fn repo(&self, owner: impl Into<String>, repo: impl Into<String>) -> RepoHandler<'_> {
        RepoHandler {
            client: self,
            owner: owner.into(),
            repo: repo.into(),
        }
    }

    /// Operations on a single notification thread.
    pub fn thread(&self, id: impl Into<ThreadId>) -> ThreadHandler<'_> {
        ThreadHandler {
            client: self,
            id: id.into(),
        }
    }

    /// Join a relative API path onto the configured base URL.
    pub(crate) fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url.join(path).map_err(|_| Error::InvalidBaseUrl)
    }

    /// Start a request for `method` + `url`. Authentication is attached by [`execute`].
    pub(crate) fn request(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        self.http.request(method, url)
    }

    /// Attach authentication and send a request, returning the raw response.
    pub(crate) async fn execute(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let token = self.auth.bearer().await?;
        let response = request.bearer_auth(token.expose_secret()).send().await?;
        Ok(response)
    }

    /// GET a listing URL, optionally conditional, and interpret it.
    pub(crate) async fn execute_list<T>(
        &self,
        url: Url,
        if_modified_since: Option<&str>,
    ) -> Result<Listing<T>>
    where
        T: DeserializeOwned,
    {
        let mut request = self.request(Method::GET, url);
        if let Some(value) = if_modified_since {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, value);
        }
        let response = self.execute(request).await?;
        self.interpret_list::<T>(response).await
    }

    /// Interpret a listing response, treating `304` as success rather than an error.
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
            _ => Err(self.error_for(status, resp).await),
        }
    }

    /// Interpret a response whose `200` body is a single `T`.
    pub(crate) async fn interpret_one<T>(&self, resp: reqwest::Response) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let status = resp.status();
        if status == StatusCode::OK {
            let body = resp.text().await?;
            serde_json::from_str::<T>(&body).map_err(|source| Error::Deserialize { source, body })
        } else {
            Err(self.error_for(status, resp).await)
        }
    }

    /// Interpret a response that carries no body on success (mark read/done, etc.).
    /// Any 2xx (including `202 Accepted` for async processing) and `304` map to `Ok(())`.
    pub(crate) async fn interpret_unit(&self, resp: reqwest::Response) -> Result<()> {
        let status = resp.status();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            Ok(())
        } else {
            Err(self.error_for(status, resp).await)
        }
    }

    /// Map a non-success status to the right [`Error`], distinguishing rate limits.
    async fn error_for(&self, status: StatusCode, resp: reqwest::Response) -> Error {
        match status {
            StatusCode::UNAUTHORIZED => Error::Unauthorized,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = parse_retry_after(resp.headers());
                let rate_limit = RateLimit::from_headers(resp.headers());
                if retry_after.is_some() {
                    Error::RateLimited {
                        kind: RateLimitKind::Secondary,
                        retry_after,
                        reset_at: rate_limit.reset_at,
                    }
                } else if rate_limit.remaining == Some(0) {
                    Error::RateLimited {
                        kind: RateLimitKind::Primary,
                        retry_after: None,
                        reset_at: rate_limit.reset_at,
                    }
                } else {
                    api_error(status, resp).await
                }
            }
            _ => api_error(status, resp).await,
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

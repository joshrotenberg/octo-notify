//! # octo-notify
//!
//! A focused Rust client for the GitHub Notifications API: complete, typed coverage of every
//! endpoint, plus a polling engine for building applications on the notifications inbox. It is
//! not a general GitHub client.
//!
//! ## Layers
//!
//! 1. **Typed client** ([`Client`]): every Notifications endpoint, conditional requests
//!    (`If-Modified-Since` / `304`), rate-limit accounting, pagination, and forward-compatible
//!    models.
//! 2. **App engine** ([`Poller`](crate::app::Poller)): the inbox as an async
//!    [`Stream`](futures::Stream) of [`Event`](crate::app::Event)s, with client-side filters.
//! 3. **State** ([`StateStore`](crate::app::StateStore)): dedupe across restarts.
//!
//! Layers 2 and 3 need the `stream` feature, which is on by default.
//!
//! ## Authentication
//!
//! The notifications inbox is user-scoped, so authentication is a single bearer token. A classic
//! PAT needs the `notifications` scope (or `repo` to also read issue and commit subjects); a
//! fine-grained PAT needs the read-only "Notifications" account permission.
//!
//! ```no_run
//! use octo_notify::{Auth, Client};
//!
//! # fn main() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?; // GITHUB_TOKEN or GH_TOKEN
//! let explicit = Client::new(Auth::token("ghp_example"))?;
//! # let _ = (client, explicit);
//! # Ok(())
//! # }
//! ```
//!
//! Tokens come from a [`TokenProvider`], so credentials that expire can refresh themselves. The
//! `token-refresh` feature adds `RefreshingToken` for GitHub App user-to-server tokens. Override
//! the base URL for GitHub Enterprise Server:
//!
//! ```no_run
//! # use octo_notify::{Auth, Client};
//! # fn main() -> octo_notify::Result<()> {
//! let client = Client::builder()
//!     .auth(Auth::from_env()?)
//!     .base_url("https://ghe.example.com/api/v3")
//!     .user_agent("my-app/1.0")
//!     .build()?;
//! # let _ = client;
//! # Ok(())
//! # }
//! ```
//!
//! ## Listing notifications
//!
//! `client.notifications()` lists the inbox; `client.repo(owner, name).notifications()` scopes to
//! one repository. A single `send()` returns a [`Listing`] (which may be [`Listing::NotModified`]
//! when the request is conditional); `all()` collects every page and `stream()` yields items
//! across pages.
//!
//! ```no_run
//! use octo_notify::{Auth, Client, Listing, Reason};
//!
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let listing = client.notifications().list().participating(true).send().await?;
//! if let Listing::Modified(page) = listing {
//!     for n in &page.items {
//!         if matches!(n.reason, Reason::Mention) {
//!             println!("{} - {}", n.repository.full_name, n.subject.title);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Threads and subscriptions
//!
//! `client.thread(id)` operates on one thread: fetch it, mark it read or done (one-way), and
//! manage its subscription.
//!
//! ```no_run
//! # use octo_notify::{Auth, Client};
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let thread = client.thread("123456789");
//! let notification = thread.get().await?;
//! thread.mark_read().await?;
//! thread.set_subscription(true).await?; // ignore future notifications
//! # let _ = notification;
//! # Ok(())
//! # }
//! ```
//!
//! ## Watching the inbox
//!
//! [`Client::poller`] builds a [`Poller`](crate::app::Poller) that yields an async stream of
//! [`Event`](crate::app::Event)s. By default it watches the whole inbox, obeys the server poll
//! interval, and dedupes through an in-memory store. Filter by reason, subject type, repository,
//! or a predicate.
//!
//! ```no_run
//! use futures::StreamExt;
//! use octo_notify::app::Event;
//! use octo_notify::{Auth, Client, Reason};
//!
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let mut events = Box::pin(client.poller().reasons([Reason::ReviewRequested]).build().stream());
//! while let Some(event) = events.next().await {
//!     match event? {
//!         Event::New(n) => println!("new: {}", n.subject.title),
//!         Event::Updated(n) => println!("updated: {}", n.subject.title),
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Persist state across restarts with the `file-store` feature and `JsonFileStore`:
//!
//! ```ignore
//! use octo_notify::app::JsonFileStore;
//!
//! let store = JsonFileStore::open("/var/cache/octo-notify.json")?;
//! let poller = client.poller().store(store).build();
//! ```
//!
//! ### Robustness contract
//!
//! - **Transient vs fatal errors:** network blips, 5xx, and rate limits back off and retry; only
//!   fatal errors (for example `401`) end the stream.
//! - **At-least-once delivery:** events emit in ascending `updated_at`; per-thread seen state
//!   commits after delivery and the `Last-Modified` watermark advances only after the full tick,
//!   so a crash re-emits rather than drops. Dedupe downstream on `(id, updated_at)`.
//! - **Pagination vs conditional requests:** the `If-Modified-Since` check applies to page 1; on
//!   a change, all pages are fetched before classification, so the inbox is never half-read.
//! - **Shutdown:** drop the stream, or pass a `CancellationToken` for cooperative shutdown.
//!
//! ## Bulk actions
//!
//! Mark many threads read or done concurrently, with a bounded worker count and per-item results.
//!
//! ```no_run
//! # use octo_notify::{Auth, Client, ThreadId};
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let ids = [ThreadId::from("1"), ThreadId::from("2")];
//! for (id, result) in client.mark_done_each(ids, 8).await {
//!     if let Err(e) = result {
//!         eprintln!("{id}: {e}");
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Errors and rate limits
//!
//! Every fallible call returns [`Result`]. [`Error`] distinguishes API errors, primary and
//! secondary rate limits ([`Error::RateLimited`] with [`RateLimitKind`]), authentication
//! failures, and deserialization problems. A `304` is success, not an error: it surfaces as
//! [`Listing::NotModified`]. With the `retry` feature, `RetryPolicy` makes one-shot calls wait
//! out rate limits automatically.
//!
//! ## Feature flags
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `rustls` | yes | TLS via rustls |
//! | `native-tls` | no | TLS via the platform's native stack |
//! | `stream` | yes | The poller engine and `stream()` pagination |
//! | `file-store` | no | `JsonFileStore`, file-backed state for cross-restart dedupe |
//! | `sqlite-store` | no | `SqliteStore`, SQLite-backed state |
//! | `token-refresh` | no | `RefreshingToken` for expiring credentials |
//! | `retry` | no | `RetryPolicy` for auto-retrying rate-limited calls |
//! | `tracing` | no | Structured `tracing` instrumentation of requests and the poller |
//! | `cli` | no | The `octo-notify` command-line binary (`cargo install octo-notify --features cli`) |
//!
//! With the `tracing` feature on, install any `tracing` subscriber (for example
//! `tracing-subscriber`) in your application to see request, rate-limit, and poller events.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Emit a `tracing` debug event when the `tracing` feature is on; a no-op otherwise.
macro_rules! tdebug {
    ($($arg:tt)*) => {{
        #[cfg(feature = "tracing")]
        ::tracing::debug!($($arg)*);
    }};
}
/// Emit a `tracing` warn event when the `tracing` feature is on; a no-op otherwise.
macro_rules! twarn {
    ($($arg:tt)*) => {{
        #[cfg(feature = "tracing")]
        ::tracing::warn!($($arg)*);
    }};
}

#[cfg(feature = "stream")]
pub mod app;
pub mod auth;
mod client;
pub mod endpoints;
pub mod error;
pub mod models;
pub mod pagination;
pub mod rate_limit;

#[cfg(feature = "token-refresh")]
pub use auth::RefreshingToken;
pub use auth::{Auth, TokenProvider};
#[cfg(feature = "retry")]
pub use client::RetryPolicy;
pub use client::{Client, ClientBuilder};
pub use endpoints::{
    ListNotifications, MarkAllRead, NotificationsHandler, RepoHandler, RepoNotificationsHandler,
    ThreadHandler,
};
pub use error::{Error, RateLimitKind, Result};
pub use models::{
    MinimalRepository, Notification, Reason, SimpleUser, Subject, SubjectType, ThreadId,
    ThreadSubscription,
};
pub use pagination::{Listing, NotModified, Page};
pub use rate_limit::RateLimit;
pub use secrecy::SecretString;

/// Common imports for downstream apps: `use octo_notify::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Auth, Client, Error, Listing, Notification, Page, Reason, Result, Subject, SubjectType,
    };
}

//! # octo-notify
//!
//! A focused Rust client for the GitHub Notifications API.
//!
//! This crate does one thing: work with a GitHub user's notifications inbox, completely and
//! correctly. It is not a general GitHub client.
//!
//! ## Layers
//!
//! 1. **Typed client** ([`Client`]) covering every Notifications endpoint, with conditional
//!    requests (`If-Modified-Since` / `304`), rate-limit accounting, pagination, and
//!    forward-compatible models.
//! 2. **App engine** ([`Poller`](crate::app::Poller)) turning the inbox into an async
//!    [`Stream`](futures::Stream) of [`Event`](crate::app::Event)s, with client-side filters.
//! 3. **State** ([`StateStore`](crate::app::StateStore)) so a long-running poller dedupes
//!    across restarts.
//!
//! ## Listing example
//!
//! ```no_run
//! use octo_notify::{Auth, Client, Listing, Reason};
//!
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let listing = client
//!     .notifications()
//!     .list()
//!     .participating(true)
//!     .send()
//!     .await?;
//!
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
//! ## Watching example
//!
//! ```no_run
//! use futures::StreamExt;
//! use octo_notify::{Auth, Client};
//! use octo_notify::app::Event;
//!
//! # async fn run() -> octo_notify::Result<()> {
//! let client = Client::new(Auth::from_env()?)?;
//! let mut events = Box::pin(client.poller().build().stream());
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
//! ## Robustness contract (the poller)
//!
//! - **Transient vs fatal errors:** network blips, 5xx, and rate limits are retried with
//!   backoff; only fatal errors (e.g. `401`) end the stream.
//! - **At-least-once delivery:** events emit in ascending `updated_at`; per-thread seen state
//!   commits after an event is delivered and the `Last-Modified` watermark advances only after
//!   the full tick, so a crash re-emits rather than drops. Dedupe downstream on `(id, updated_at)`.
//! - **Pagination vs conditional requests:** the `If-Modified-Since` check applies to page 1;
//!   on a change, all pages are fetched before classification so the inbox is never half-read.
//! - **Shutdown:** drop the stream, or pass a `CancellationToken` for cooperative shutdown.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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

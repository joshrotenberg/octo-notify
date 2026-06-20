//! # octo-notify
//!
//! A focused Rust client for the GitHub Notifications API.
//!
//! This crate does one thing: work with a GitHub user's notifications inbox, completely and
//! correctly. It is not a general GitHub client.
//!
//! ## Status
//!
//! Milestone 1: the typed client foundation plus `GET /notifications` with conditional
//! requests (`If-Modified-Since` / `304`), rate-limit accounting, and forward-compatible
//! models. The poller, full endpoint coverage, and state stores follow in later milestones.
//!
//! ## Example
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
//!             println!("{} — {}", n.repository.full_name, n.subject.title);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod auth;
mod client;
pub mod endpoints;
pub mod error;
pub mod models;
pub mod pagination;
pub mod rate_limit;

pub use auth::{Auth, TokenProvider};
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

/// Common imports for downstream apps: `use octo_notify::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Auth, Client, Error, Listing, Notification, Page, Reason, Result, Subject, SubjectType,
    };
}

//! Thread subscription model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

/// A user's subscription to a notification thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSubscription {
    /// Whether the user is subscribed to the thread.
    pub subscribed: bool,
    /// Whether the thread is ignored (all notifications suppressed).
    pub ignored: bool,
    /// Why the user is subscribed, if known.
    #[serde(default)]
    pub reason: Option<String>,
    /// When the subscription was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// API URL of the subscription.
    pub url: Url,
    /// API URL of the thread.
    #[serde(default)]
    pub thread_url: Option<Url>,
    /// API URL of the repository.
    #[serde(default)]
    pub repository_url: Option<Url>,
}

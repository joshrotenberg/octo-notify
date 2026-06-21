//! The notification thread model.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{MinimalRepository, Reason, Subject};

/// Identifier for a notification thread.
///
/// The API returns this as a string; the thread endpoints accept it in their path.
///
/// ```
/// use octo_notify::ThreadId;
/// assert_eq!(ThreadId::from(42u64).as_str(), "42");
/// assert_eq!(ThreadId::from("123").to_string(), "123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(pub String);

impl ThreadId {
    /// Borrow the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ThreadId {
    fn from(s: String) -> Self {
        ThreadId(s)
    }
}

impl From<&str> for ThreadId {
    fn from(s: &str) -> Self {
        ThreadId(s.to_owned())
    }
}

impl From<u64> for ThreadId {
    fn from(n: u64) -> Self {
        ThreadId(n.to_string())
    }
}

/// A single notification (a "thread") in the user's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// The thread id.
    pub id: ThreadId,
    /// The repository the notification belongs to.
    pub repository: MinimalRepository,
    /// What the notification is about.
    pub subject: Subject,
    /// Why the notification was delivered.
    pub reason: Reason,
    /// Whether the thread is unread.
    pub unread: bool,
    /// When the thread was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the thread was last read, if ever.
    pub last_read_at: Option<DateTime<Utc>>,
    /// API URL of the thread.
    pub url: Url,
    /// API URL of the thread's subscription.
    pub subscription_url: Url,
}

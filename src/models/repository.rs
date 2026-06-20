//! The minimal repository and user shapes carried in a notification payload.
//!
//! These are deliberately partial: a notification payload embeds only a subset of the full
//! repository/user objects, and unknown JSON fields are ignored.

use serde::{Deserialize, Serialize};
use url::Url;

/// The subset of a repository carried inside a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalRepository {
    /// Numeric repository id.
    pub id: u64,
    /// Repository name (without owner).
    pub name: String,
    /// `owner/name`.
    pub full_name: String,
    /// The repository owner.
    pub owner: SimpleUser,
    /// Whether the repository is private.
    pub private: bool,
    /// Web URL of the repository.
    pub html_url: Url,
    /// Whether the repository is a fork.
    pub fork: bool,
}

/// The subset of a user/organization carried inside a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleUser {
    /// Login handle.
    pub login: String,
    /// Numeric user id.
    pub id: u64,
    /// Web URL of the user's profile.
    pub html_url: Url,
    /// Account type, e.g. `User` or `Organization`.
    #[serde(rename = "type")]
    pub kind: String,
}

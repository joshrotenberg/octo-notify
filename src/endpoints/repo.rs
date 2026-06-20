//! Repository-scoped notification endpoints.

use crate::client::Client;
use crate::endpoints::notifications::{ListNotifications, MarkAllRead};

/// Operations scoped to a single repository.
pub struct RepoHandler<'a> {
    pub(crate) client: &'a Client,
    pub(crate) owner: String,
    pub(crate) repo: String,
}

impl<'a> RepoHandler<'a> {
    /// Notification operations for this repository.
    pub fn notifications(&self) -> RepoNotificationsHandler<'_> {
        RepoNotificationsHandler {
            client: self.client,
            path: format!("repos/{}/{}/notifications", self.owner, self.repo),
        }
    }
}

/// Entry point for one repository's notifications.
pub struct RepoNotificationsHandler<'a> {
    client: &'a Client,
    path: String,
}

impl<'a> RepoNotificationsHandler<'a> {
    /// List this repository's notifications (`GET /repos/{owner}/{repo}/notifications`).
    pub fn list(&self) -> ListNotifications<'a> {
        ListNotifications::new(self.client, self.path.clone())
    }

    /// Mark this repository's notifications as read
    /// (`PUT /repos/{owner}/{repo}/notifications`).
    pub fn mark_all_read(&self) -> MarkAllRead<'a> {
        MarkAllRead::new(self.client, self.path.clone())
    }
}

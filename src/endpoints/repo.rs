//! Repository-scoped notification endpoints.

use reqwest::Method;
use serde::Serialize;

use crate::client::Client;
use crate::endpoints::notifications::{ListNotifications, MarkAllRead};
use crate::error::Result;
use crate::models::RepositorySubscription;

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

    fn subscription_path(&self) -> String {
        format!("repos/{}/{}/subscription", self.owner, self.repo)
    }

    /// Get this repository's subscription (`GET /repos/{owner}/{repo}/subscription`).
    ///
    /// The API returns `404 Not Found` (surfaced as [`Error::Api`] with that status) when
    /// you are neither watching nor ignoring the repository.
    ///
    /// [`Error::Api`]: crate::Error::Api
    pub async fn subscription(&self) -> Result<RepositorySubscription> {
        let url = self.client.endpoint(&self.subscription_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::GET, url))
            .await?;
        self.client.interpret_one(response).await
    }

    /// Set this repository's subscription (`PUT /repos/{owner}/{repo}/subscription`).
    ///
    /// `subscribed = true` watches the repository (notifications for all activity);
    /// `ignored = true` suppresses all notifications. Setting both to `false` is equivalent
    /// to not subscribing. See [`subscribe`](Self::subscribe) and [`ignore`](Self::ignore)
    /// for the common cases.
    pub async fn set_subscription(
        &self,
        subscribed: bool,
        ignored: bool,
    ) -> Result<RepositorySubscription> {
        let url = self.client.endpoint(&self.subscription_path())?;
        let request = self
            .client
            .request(Method::PUT, url)
            .json(&SetSubscriptionBody {
                subscribed,
                ignored,
            });
        let response = self.client.execute(request).await?;
        self.client.interpret_one(response).await
    }

    /// Watch this repository, receiving notifications for all of its activity.
    ///
    /// Convenience for `set_subscription(true, false)`.
    pub async fn subscribe(&self) -> Result<RepositorySubscription> {
        self.set_subscription(true, false).await
    }

    /// Ignore this repository, suppressing all of its notifications.
    ///
    /// Convenience for `set_subscription(false, true)`.
    pub async fn ignore(&self) -> Result<RepositorySubscription> {
        self.set_subscription(false, true).await
    }

    /// Delete this repository's subscription, stopping watching or ignoring
    /// (`DELETE /repos/{owner}/{repo}/subscription`).
    pub async fn delete_subscription(&self) -> Result<()> {
        let url = self.client.endpoint(&self.subscription_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::DELETE, url))
            .await?;
        self.client.interpret_unit(response).await
    }
}

#[derive(Serialize)]
struct SetSubscriptionBody {
    subscribed: bool,
    ignored: bool,
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

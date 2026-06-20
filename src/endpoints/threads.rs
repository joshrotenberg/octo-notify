//! Single-thread endpoints: get, mark read/done, and subscription management.

use reqwest::Method;
use serde::Serialize;

use crate::client::Client;
use crate::error::Result;
use crate::models::{Notification, ThreadId, ThreadSubscription};

/// Operations on one notification thread.
pub struct ThreadHandler<'a> {
    pub(crate) client: &'a Client,
    pub(crate) id: ThreadId,
}

impl ThreadHandler<'_> {
    fn thread_path(&self) -> String {
        format!("notifications/threads/{}", self.id)
    }

    fn subscription_path(&self) -> String {
        format!("notifications/threads/{}/subscription", self.id)
    }

    /// The thread id this handler targets.
    pub fn id(&self) -> &ThreadId {
        &self.id
    }

    /// Get the thread (`GET /notifications/threads/{id}`).
    pub async fn get(&self) -> Result<Notification> {
        let url = self.client.endpoint(&self.thread_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::GET, url))
            .await?;
        self.client.interpret_one(response).await
    }

    /// Mark the thread as read (`PATCH /notifications/threads/{id}`).
    pub async fn mark_read(&self) -> Result<()> {
        let url = self.client.endpoint(&self.thread_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::PATCH, url))
            .await?;
        self.client.interpret_unit(response).await
    }

    /// Mark the thread as done, removing it from the inbox
    /// (`DELETE /notifications/threads/{id}`).
    ///
    /// This is one-way: the API has no "un-done" operation.
    pub async fn mark_done(&self) -> Result<()> {
        let url = self.client.endpoint(&self.thread_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::DELETE, url))
            .await?;
        self.client.interpret_unit(response).await
    }

    /// Get the thread subscription (`GET /notifications/threads/{id}/subscription`).
    pub async fn subscription(&self) -> Result<ThreadSubscription> {
        let url = self.client.endpoint(&self.subscription_path())?;
        let response = self
            .client
            .execute(self.client.request(Method::GET, url))
            .await?;
        self.client.interpret_one(response).await
    }

    /// Set the thread subscription (`PUT /notifications/threads/{id}/subscription`).
    ///
    /// Pass `ignored = true` to suppress all future notifications from the thread.
    pub async fn set_subscription(&self, ignored: bool) -> Result<ThreadSubscription> {
        let url = self.client.endpoint(&self.subscription_path())?;
        let request = self
            .client
            .request(Method::PUT, url)
            .json(&SetSubscriptionBody { ignored });
        let response = self.client.execute(request).await?;
        self.client.interpret_one(response).await
    }

    /// Delete the thread subscription, muting it until you participate again
    /// (`DELETE /notifications/threads/{id}/subscription`).
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
    ignored: bool,
}

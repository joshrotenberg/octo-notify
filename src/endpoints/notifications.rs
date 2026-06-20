//! The authenticated user's notification inbox: `GET /notifications`.

use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;

use crate::client::Client;
use crate::error::Result;
use crate::models::Notification;
use crate::pagination::Listing;

/// Entry point for inbox-level notification operations.
pub struct NotificationsHandler<'a> {
    pub(crate) client: &'a Client,
}

impl<'a> NotificationsHandler<'a> {
    /// Build a request to list notifications (`GET /notifications`).
    pub fn list(&self) -> ListNotifications<'a> {
        ListNotifications::new(self.client)
    }
}

/// Builder for `GET /notifications`.
///
/// Set the `If-Modified-Since` header via [`if_modified_since`](Self::if_modified_since)
/// to make the request conditional; a `304` then comes back as
/// [`Listing::NotModified`](crate::Listing::NotModified) rather than re-downloading data.
pub struct ListNotifications<'a> {
    client: &'a Client,
    all: Option<bool>,
    participating: Option<bool>,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    per_page: Option<u8>,
    page: Option<u32>,
    if_modified_since: Option<String>,
}

impl<'a> ListNotifications<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        ListNotifications {
            client,
            all: None,
            participating: None,
            since: None,
            before: None,
            per_page: None,
            page: None,
            if_modified_since: None,
        }
    }

    /// Include notifications already marked as read (`all`).
    pub fn all(mut self, all: bool) -> Self {
        self.all = Some(all);
        self
    }

    /// Only notifications the user is directly participating in (`participating`).
    pub fn participating(mut self, participating: bool) -> Self {
        self.participating = Some(participating);
        self
    }

    /// Only notifications updated after this time (`since`).
    pub fn since(mut self, since: DateTime<Utc>) -> Self {
        self.since = Some(since);
        self
    }

    /// Only notifications updated before this time (`before`).
    pub fn before(mut self, before: DateTime<Utc>) -> Self {
        self.before = Some(before);
        self
    }

    /// Results per page (`per_page`); the inbox endpoint caps this at 50.
    pub fn per_page(mut self, per_page: u8) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Page number (`page`), 1-based.
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Make the request conditional with an `If-Modified-Since` value, typically the
    /// `last_modified` returned by a previous call.
    pub fn if_modified_since(mut self, value: impl Into<String>) -> Self {
        self.if_modified_since = Some(value.into());
        self
    }

    /// Send the request.
    pub async fn send(self) -> Result<Listing<Notification>> {
        let mut url = self.client.endpoint("notifications")?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(all) = self.all {
                query.append_pair("all", bool_str(all));
            }
            if let Some(participating) = self.participating {
                query.append_pair("participating", bool_str(participating));
            }
            if let Some(since) = self.since {
                query.append_pair("since", &since.to_rfc3339());
            }
            if let Some(before) = self.before {
                query.append_pair("before", &before.to_rfc3339());
            }
            if let Some(per_page) = self.per_page {
                query.append_pair("per_page", &per_page.to_string());
            }
            if let Some(page) = self.page {
                query.append_pair("page", &page.to_string());
            }
        }

        let token = self.client.auth().bearer().await?;
        let mut request = self
            .client
            .http()
            .get(url)
            .bearer_auth(token.expose_secret());
        if let Some(value) = &self.if_modified_since {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, value);
        }

        let response = request.send().await?;
        self.client.interpret_list::<Notification>(response).await
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

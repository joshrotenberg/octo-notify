//! Inbox-level notification endpoints, reused for repository scope.
//!
//! [`ListNotifications`] and [`MarkAllRead`] are path-parameterized so the same builders
//! serve both `GET/PUT /notifications` and `GET/PUT /repos/{owner}/{repo}/notifications`.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Serialize;
use url::Url;

use crate::client::Client;
use crate::error::Result;
use crate::models::Notification;
use crate::pagination::Listing;

/// Entry point for the authenticated user's whole notification inbox.
pub struct NotificationsHandler<'a> {
    pub(crate) client: &'a Client,
}

impl<'a> NotificationsHandler<'a> {
    /// List notifications (`GET /notifications`).
    pub fn list(&self) -> ListNotifications<'a> {
        ListNotifications::new(self.client, "notifications".to_owned())
    }

    /// Mark all notifications as read (`PUT /notifications`).
    pub fn mark_all_read(&self) -> MarkAllRead<'a> {
        MarkAllRead::new(self.client, "notifications".to_owned())
    }
}

/// Builder for a notifications listing.
///
/// Set [`if_modified_since`](Self::if_modified_since) to make the request conditional; a
/// `304` then comes back as [`Listing::NotModified`]. Use
/// [`all`](Self::all) or [`stream`](Self::stream) to transparently follow pagination.
pub struct ListNotifications<'a> {
    client: &'a Client,
    path: String,
    include_read: Option<bool>,
    participating: Option<bool>,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    per_page: Option<u8>,
    page: Option<u32>,
    if_modified_since: Option<String>,
}

impl<'a> ListNotifications<'a> {
    pub(crate) fn new(client: &'a Client, path: String) -> Self {
        ListNotifications {
            client,
            path,
            include_read: None,
            participating: None,
            since: None,
            before: None,
            per_page: None,
            page: None,
            if_modified_since: None,
        }
    }

    /// Include notifications already marked as read (the `all` query parameter).
    pub fn include_read(mut self, include_read: bool) -> Self {
        self.include_read = Some(include_read);
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

    /// Results per page (`per_page`); inbox scope caps at 50, repo scope at 100.
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

    fn first_url(&self) -> Result<Url> {
        let mut url = self.client.endpoint(&self.path)?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(include_read) = self.include_read {
                query.append_pair("all", bool_str(include_read));
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
        Ok(url)
    }

    /// Send the request for a single page.
    pub async fn send(self) -> Result<Listing<Notification>> {
        let url = self.first_url()?;
        self.client
            .execute_list::<Notification>(url, self.if_modified_since.as_deref())
            .await
    }

    /// Fetch every page, following `Link: rel="next"`, and collect the results.
    ///
    /// Pagination is unconditional: any `if_modified_since` set on the builder is ignored
    /// here, since the intent is to retrieve the full current set.
    pub async fn all(self) -> Result<Vec<Notification>> {
        let mut out = Vec::new();
        let mut next = Some(self.first_url()?);
        while let Some(url) = next {
            match self.client.execute_list::<Notification>(url, None).await? {
                Listing::Modified(page) => {
                    out.extend(page.items);
                    next = page.next;
                }
                Listing::NotModified(_) => break,
            }
        }
        Ok(out)
    }

    /// Stream every notification across all pages, following `Link: rel="next"`.
    #[cfg(feature = "stream")]
    pub fn stream(self) -> impl futures::Stream<Item = Result<Notification>> + 'a {
        let client = self.client;
        let first = self.first_url();
        async_stream::try_stream! {
            let mut next = Some(first?);
            while let Some(url) = next {
                let page = match client.execute_list::<Notification>(url, None).await? {
                    Listing::Modified(page) => page,
                    Listing::NotModified(_) => break,
                };
                for item in page.items {
                    yield item;
                }
                next = page.next;
            }
        }
    }
}

/// Builder for marking notifications as read (`PUT /notifications` or the repo variant).
pub struct MarkAllRead<'a> {
    client: &'a Client,
    path: String,
    last_read_at: Option<DateTime<Utc>>,
    read: Option<bool>,
}

impl<'a> MarkAllRead<'a> {
    pub(crate) fn new(client: &'a Client, path: String) -> Self {
        MarkAllRead {
            client,
            path,
            last_read_at: None,
            read: None,
        }
    }

    /// Only mark notifications updated up to this time (`last_read_at`).
    pub fn last_read_at(mut self, last_read_at: DateTime<Utc>) -> Self {
        self.last_read_at = Some(last_read_at);
        self
    }

    /// Set the `read` flag (inbox scope only).
    pub fn read(mut self, read: bool) -> Self {
        self.read = Some(read);
        self
    }

    /// Send the request. Large inboxes may be processed asynchronously (`202 Accepted`),
    /// in which case the change is not immediately reflected by a subsequent list.
    pub async fn send(self) -> Result<()> {
        let url = self.client.endpoint(&self.path)?;
        let body = MarkAllReadBody {
            last_read_at: self.last_read_at.map(|t| t.to_rfc3339()),
            read: self.read,
        };
        let request = self.client.request(Method::PUT, url).json(&body);
        let response = self.client.execute(request).await?;
        self.client.interpret_unit(response).await
    }
}

#[derive(Serialize)]
struct MarkAllReadBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_read_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read: Option<bool>,
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

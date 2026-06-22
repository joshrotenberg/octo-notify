//! The authenticated user's watched repositories (`GET /user/subscriptions`).
//!
//! This is GitHub's "Watching" listing: the repositories you are subscribed to and so
//! receive notifications for. It is the inventory counterpart to
//! [`RepoHandler`](crate::RepoHandler)'s per-repository subscription management.

use url::Url;

use crate::client::Client;
use crate::error::Result;
use crate::models::MinimalRepository;
use crate::pagination::Listing;

/// Builder for listing the authenticated user's watched repositories
/// (`GET /user/subscriptions`).
///
/// Use [`all`](Self::all) or [`stream`](Self::stream) to transparently follow pagination, or
/// [`send`](Self::send) for a single page.
pub struct ListSubscriptions<'a> {
    client: &'a Client,
    per_page: Option<u8>,
    page: Option<u32>,
}

impl<'a> ListSubscriptions<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        ListSubscriptions {
            client,
            per_page: None,
            page: None,
        }
    }

    /// Results per page (`per_page`); caps at 100.
    pub fn per_page(mut self, per_page: u8) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Page number (`page`), 1-based.
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    fn first_url(&self) -> Result<Url> {
        let mut url = self.client.endpoint("user/subscriptions")?;
        {
            let mut query = url.query_pairs_mut();
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
    pub async fn send(self) -> Result<Listing<MinimalRepository>> {
        let url = self.first_url()?;
        self.client
            .execute_list::<MinimalRepository>(url, None)
            .await
    }

    /// Fetch every page, following `Link: rel="next"`, and collect the results.
    pub async fn all(self) -> Result<Vec<MinimalRepository>> {
        let mut out = Vec::new();
        let mut next = Some(self.first_url()?);
        while let Some(url) = next {
            match self
                .client
                .execute_list::<MinimalRepository>(url, None)
                .await?
            {
                Listing::Modified(page) => {
                    out.extend(page.items);
                    next = page.next;
                }
                Listing::NotModified(_) => break,
            }
        }
        Ok(out)
    }

    /// Stream every watched repository across all pages, following `Link: rel="next"`.
    #[cfg(feature = "stream")]
    pub fn stream(self) -> impl futures::Stream<Item = Result<MinimalRepository>> + 'a {
        let client = self.client;
        let first = self.first_url();
        async_stream::try_stream! {
            let mut next = Some(first?);
            while let Some(url) = next {
                let page = match client.execute_list::<MinimalRepository>(url, None).await? {
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

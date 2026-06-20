//! Listing results, pagination, and conditional-request outcomes.

use std::time::Duration;

use reqwest::header::HeaderMap;
use url::Url;

use crate::rate_limit::RateLimit;

/// One page of a listing response (`200 OK`).
#[derive(Debug, Clone)]
pub struct Page<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Server-requested minimum poll interval (`X-Poll-Interval`).
    pub poll_interval: Option<Duration>,
    /// The `Last-Modified` value to feed back as `If-Modified-Since` next time.
    pub last_modified: Option<String>,
    /// Rate-limit snapshot from this response.
    pub rate_limit: RateLimit,
    pub(crate) next: Option<Url>,
}

impl<T> Page<T> {
    /// Whether a `Link: rel="next"` page is available.
    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }

    /// The URL of the next page, if any.
    pub fn next_url(&self) -> Option<&Url> {
        self.next.as_ref()
    }
}

/// The result of a conditional request that returned `304 Not Modified`.
#[derive(Debug, Clone)]
pub struct NotModified {
    /// Server-requested minimum poll interval (`X-Poll-Interval`).
    pub poll_interval: Option<Duration>,
    /// The `Last-Modified` value (unchanged since the request's `If-Modified-Since`).
    pub last_modified: Option<String>,
    /// Rate-limit snapshot. A `304` does not consume primary budget.
    pub rate_limit: RateLimit,
}

/// Outcome of a listing request: fresh data, or "nothing changed".
///
/// A `304` is reported as [`Listing::NotModified`] rather than an error, because for a
/// polling library "nothing changed" is the common, successful case.
#[derive(Debug, Clone)]
pub enum Listing<T> {
    /// `200 OK` with a page of results.
    Modified(Page<T>),
    /// `304 Not Modified` in response to a conditional request.
    NotModified(NotModified),
}

impl<T> Listing<T> {
    /// `true` if this carries fresh data.
    pub fn is_modified(&self) -> bool {
        matches!(self, Listing::Modified(_))
    }

    /// Borrow the page, if data was returned.
    pub fn page(&self) -> Option<&Page<T>> {
        match self {
            Listing::Modified(page) => Some(page),
            Listing::NotModified(_) => None,
        }
    }

    /// Consume into the page, if data was returned.
    pub fn into_page(self) -> Option<Page<T>> {
        match self {
            Listing::Modified(page) => Some(page),
            Listing::NotModified(_) => None,
        }
    }

    /// The poll interval, regardless of outcome.
    pub fn poll_interval(&self) -> Option<Duration> {
        match self {
            Listing::Modified(page) => page.poll_interval,
            Listing::NotModified(nm) => nm.poll_interval,
        }
    }

    /// The `Last-Modified` value, regardless of outcome.
    pub fn last_modified(&self) -> Option<&str> {
        match self {
            Listing::Modified(page) => page.last_modified.as_deref(),
            Listing::NotModified(nm) => nm.last_modified.as_deref(),
        }
    }

    /// The rate-limit snapshot, regardless of outcome.
    pub fn rate_limit(&self) -> &RateLimit {
        match self {
            Listing::Modified(page) => &page.rate_limit,
            Listing::NotModified(nm) => &nm.rate_limit,
        }
    }
}

/// Parse the `rel="next"` URL out of a `Link` header, if present.
pub(crate) fn parse_link_next(headers: &HeaderMap) -> Option<Url> {
    let link = headers.get(reqwest::header::LINK)?.to_str().ok()?;
    for part in link.split(',') {
        let mut segments = part.split(';');
        let url_segment = segments.next()?.trim();
        let url_str = url_segment.strip_prefix('<')?.strip_suffix('>')?;
        let is_next = segments.any(|s| s.trim() == r#"rel="next""#);
        if is_next {
            return Url::parse(url_str).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, LINK};

    #[test]
    fn parses_next_link() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LINK,
            HeaderValue::from_static(
                "<https://api.github.com/notifications?page=2>; rel=\"next\", \
                 <https://api.github.com/notifications?page=5>; rel=\"last\"",
            ),
        );
        let next = parse_link_next(&headers).unwrap();
        assert_eq!(next.as_str(), "https://api.github.com/notifications?page=2");
    }

    #[test]
    fn no_next_link_when_only_prev() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LINK,
            HeaderValue::from_static("<https://api.github.com/notifications?page=1>; rel=\"prev\""),
        );
        assert!(parse_link_next(&headers).is_none());
    }
}

//! Integration tests for `GET /notifications`, served by a mock GitHub (wiremock).

use std::time::Duration;

use octo_notify::{Auth, Client, Error, Reason, SubjectType};
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURE: &str = include_str!("fixtures/notifications_page1.json");

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests")
        .build()
        .expect("client builds")
}

#[tokio::test]
async fn lists_parses_and_reports_headers() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Poll-Interval", "60")
                .insert_header("Last-Modified", "Thu, 25 Oct 2012 15:16:27 GMT")
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4999")
                .insert_header("x-ratelimit-reset", "1700000000")
                .insert_header(
                    "Link",
                    "<https://api.github.com/notifications?page=2>; rel=\"next\"",
                )
                .set_body_raw(FIXTURE, "application/json"),
        )
        .mount(&server)
        .await;

    let listing = client_for(&server)
        .notifications()
        .list()
        .send()
        .await
        .expect("request succeeds");

    let page = listing.into_page().expect("200 yields a page");
    assert_eq!(page.items.len(), 2);

    // Known reason parses; unknown reason is captured, not rejected.
    assert_eq!(page.items[0].reason, Reason::Mention);
    assert_eq!(page.items[0].subject.kind, SubjectType::Issue);
    assert!(page.items[1].reason.is_unknown());
    assert!(page.items[1].subject.kind.is_unknown());

    // Header plumbing.
    assert_eq!(page.poll_interval, Some(Duration::from_secs(60)));
    assert_eq!(
        page.last_modified.as_deref(),
        Some("Thu, 25 Oct 2012 15:16:27 GMT")
    );
    assert_eq!(page.rate_limit.remaining, Some(4999));
    assert_eq!(page.rate_limit.limit, Some(5000));
    assert!(page.rate_limit.reset_at.is_some());

    // Pagination link parsed.
    assert!(page.has_next());
}

#[tokio::test]
async fn conditional_request_sends_if_modified_since_and_handles_304() {
    let server = MockServer::start().await;

    // Route on header presence. We assert the exact value against the recorded request
    // below, rather than via a value matcher: wiremock splits header values on commas,
    // which mangles an HTTP-date like "Thu, 25 Oct 2012 15:16:27 GMT".
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(header_exists("if-modified-since"))
        .respond_with(ResponseTemplate::new(304).insert_header("X-Poll-Interval", "120"))
        .mount(&server)
        .await;

    let last_modified = "Thu, 25 Oct 2012 15:16:27 GMT";
    let listing = client_for(&server)
        .notifications()
        .list()
        .if_modified_since(last_modified)
        .send()
        .await
        .expect("request succeeds");

    // A 304 is a success, not an error, and carries no page.
    assert!(!listing.is_modified());
    assert!(listing.page().is_none());
    assert_eq!(listing.poll_interval(), Some(Duration::from_secs(120)));

    // The exact conditional header value reached the server.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("if-modified-since")
            .map(|v| v.to_str().unwrap()),
        Some(last_modified)
    );
}

#[tokio::test]
async fn unauthorized_maps_to_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_raw(r#"{"message":"Bad credentials"}"#, "application/json"),
        )
        .mount(&server)
        .await;

    let err = client_for(&server)
        .notifications()
        .list()
        .send()
        .await
        .expect_err("401 is an error");

    assert!(matches!(err, Error::Unauthorized), "got {err:?}");
}

#[tokio::test]
async fn primary_rate_limit_maps_to_rate_limited() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1700000000")
                .set_body_raw(
                    r#"{"message":"API rate limit exceeded"}"#,
                    "application/json",
                ),
        )
        .mount(&server)
        .await;

    let err = client_for(&server)
        .notifications()
        .list()
        .send()
        .await
        .expect_err("rate limit is an error");

    match err {
        Error::RateLimited { kind, reset_at, .. } => {
            assert_eq!(kind, octo_notify::RateLimitKind::Primary);
            assert!(reset_at.is_some());
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn static_token_does_not_retry_on_401() {
    let server = MockServer::start().await;
    // expect(1): a static token must not retry on 401.
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_raw(r#"{"message":"Bad credentials"}"#, "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let err = client_for(&server)
        .notifications()
        .list()
        .send()
        .await
        .expect_err("401 surfaces");
    assert!(matches!(err, Error::Unauthorized), "got {err:?}");
}

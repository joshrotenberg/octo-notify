//! Integration tests for the optional retry policy (issue #8).
#![cfg(feature = "retry")]

use octo_notify::{Auth, Client, Error, RetryPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_with(server: &MockServer, retry: Option<RetryPolicy>) -> Client {
    let mut builder = Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests");
    if let Some(policy) = retry {
        builder = builder.retry(policy);
    }
    builder.build().expect("client builds")
}

#[tokio::test]
async fn retries_secondary_then_succeeds() {
    let server = MockServer::start().await;
    // First call: 429 with Retry-After: 0 (immediate retry). Then: 205.
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(205))
        .mount(&server)
        .await;

    let client = client_with(&server, Some(RetryPolicy::default()));
    client
        .thread("1")
        .mark_read()
        .await
        .expect("succeeds after one retry");
}

#[tokio::test]
async fn without_policy_surfaces_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .mount(&server)
        .await;

    let client = client_with(&server, None);
    let err = client
        .thread("1")
        .mark_read()
        .await
        .expect_err("no retry, errors");
    assert!(matches!(err, Error::RateLimited { .. }), "got {err:?}");
}

#[tokio::test]
async fn max_retries_respected() {
    let server = MockServer::start().await;
    // Always 429; with max_retries = 2 the request is sent exactly 3 times.
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(3)
        .mount(&server)
        .await;

    let policy = RetryPolicy {
        retry_secondary: true,
        retry_primary: false,
        max_retries: 2,
    };
    let client = client_with(&server, Some(policy));
    let err = client
        .thread("1")
        .mark_read()
        .await
        .expect_err("still rate limited after retries");
    assert!(matches!(err, Error::RateLimited { .. }), "got {err:?}");
    // Mock's .expect(3) is verified when the server drops.
}

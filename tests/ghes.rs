//! GitHub Enterprise Server coverage (issue #25): a non-default `base_url` with a path prefix
//! must be preserved when joining endpoint paths (no truncation).

use octo_notify::{Auth, Client};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ghes_client(server: &MockServer) -> Client {
    // GHES form: the API lives under /api/v3.
    Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(format!("{}/api/v3", server.uri()))
        .user_agent("octo-notify-tests")
        .build()
        .expect("client builds")
}

#[tokio::test]
async fn ghes_inbox_list_preserves_base_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/notifications"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("[]", "application/json"))
        .mount(&server)
        .await;

    let listing = ghes_client(&server)
        .notifications()
        .list()
        .send()
        .await
        .expect("ghes list");
    assert!(listing.is_modified());
}

#[tokio::test]
async fn ghes_thread_path_preserves_base_path() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(205))
        .mount(&server)
        .await;

    ghes_client(&server)
        .thread("1")
        .mark_read()
        .await
        .expect("ghes mark read");
}

#[tokio::test]
async fn ghes_repo_path_preserves_base_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octocat/hello-world/notifications"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("[]", "application/json"))
        .mount(&server)
        .await;

    let listing = ghes_client(&server)
        .repo("octocat", "hello-world")
        .notifications()
        .list()
        .send()
        .await
        .expect("ghes repo list");
    assert!(listing.is_modified());
}

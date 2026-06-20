//! Integration tests for the full endpoint surface and pagination (M2).

use futures::StreamExt;
use octo_notify::{Auth, Client};
use wiremock::matchers::{body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE1: &str = include_str!("fixtures/notifications_page1.json");
const PAGE2: &str = include_str!("fixtures/notifications_page2.json");
const THREAD: &str = include_str!("fixtures/thread.json");
const SUBSCRIPTION: &str = include_str!("fixtures/subscription.json");

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests")
        .build()
        .expect("client builds")
}

#[tokio::test]
async fn mark_all_read_sends_body_and_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/notifications"))
        .and(body_json(serde_json::json!({ "read": true })))
        .respond_with(ResponseTemplate::new(205))
        .mount(&server)
        .await;

    client_for(&server)
        .notifications()
        .mark_all_read()
        .read(true)
        .send()
        .await
        .expect("mark all read succeeds");
}

#[tokio::test]
async fn get_thread_returns_notification() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/notifications/threads/123456789"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(THREAD, "application/json"))
        .mount(&server)
        .await;

    let n = client_for(&server)
        .thread("123456789")
        .get()
        .await
        .expect("get thread");
    assert_eq!(n.subject.title, "Add the poller");
    assert!(n.subject.is_pull_request());
    assert_eq!(n.subject.issue_number(), Some(77));
}

#[tokio::test]
async fn mark_thread_read_and_done() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/42"))
        .respond_with(ResponseTemplate::new(205))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/42"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.thread(42u64).mark_read().await.expect("mark read");
    client.thread(42u64).mark_done().await.expect("mark done");
}

#[tokio::test]
async fn subscription_get_set_delete() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/notifications/threads/123456789/subscription"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SUBSCRIPTION, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/notifications/threads/123456789/subscription"))
        .and(body_json(serde_json::json!({ "ignored": true })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SUBSCRIPTION, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/123456789/subscription"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let sub = client
        .thread("123456789")
        .subscription()
        .await
        .expect("get subscription");
    assert!(sub.subscribed);

    let updated = client
        .thread("123456789")
        .set_subscription(true)
        .await
        .expect("set subscription");
    assert!(!updated.ignored); // fixture echoes the stored state

    client
        .thread("123456789")
        .delete_subscription()
        .await
        .expect("delete subscription");
}

#[tokio::test]
async fn repo_scoped_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octocat/hello-world/notifications"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(PAGE2, "application/json"))
        .mount(&server)
        .await;

    let listing = client_for(&server)
        .repo("octocat", "hello-world")
        .notifications()
        .list()
        .send()
        .await
        .expect("repo list");
    let page = listing.into_page().expect("modified");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].repository.full_name, "octocat/hello-world");
}

/// Mount a two-page inbox. The `Link` header must point back at the mock server, so it is
/// built from the server URI at runtime rather than baked into a fixture.
async fn mount_two_pages(server: &MockServer) {
    let next_link = format!("<{}/notifications?page=2>; rel=\"next\"", server.uri());
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(query_param_is_missing("page"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", next_link.as_str())
                .set_body_raw(PAGE1, "application/json"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(PAGE2, "application/json"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn pagination_all_follows_link() {
    let server = MockServer::start().await;
    mount_two_pages(&server).await;

    let all = client_for(&server)
        .notifications()
        .list()
        .all()
        .await
        .expect("collect all pages");
    assert_eq!(all.len(), 3); // 2 from page 1 + 1 from page 2
    assert_eq!(all[2].id.as_str(), "555000555");
}

#[tokio::test]
async fn pagination_stream_yields_all_items() {
    let server = MockServer::start().await;
    mount_two_pages(&server).await;

    let client = client_for(&server);
    let collected: Vec<_> = client
        .notifications()
        .list()
        .stream()
        .collect::<Vec<_>>()
        .await;
    let ids: Vec<String> = collected
        .into_iter()
        .map(|r| r.expect("item ok").id.as_str().to_owned())
        .collect();
    assert_eq!(ids, ["123456789", "987654321", "555000555"]);
}

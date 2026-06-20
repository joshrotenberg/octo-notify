//! Integration tests for bulk mark operations (issue #6).
#![cfg(feature = "stream")]

use std::collections::HashMap;

use octo_notify::{Auth, Client, ThreadId};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests")
        .build()
        .expect("client builds")
}

fn ok_by_id(results: Vec<(ThreadId, octo_notify::Result<()>)>) -> HashMap<String, bool> {
    results
        .into_iter()
        .map(|(id, r)| (id.as_str().to_owned(), r.is_ok()))
        .collect()
}

#[tokio::test]
async fn mark_read_each_reports_per_item_results() {
    let server = MockServer::start().await;
    for id in ["1", "2"] {
        Mock::given(method("PATCH"))
            .and(path(format!("/notifications/threads/{id}")))
            .respond_with(ResponseTemplate::new(205))
            .mount(&server)
            .await;
    }
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/3"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let ids: Vec<ThreadId> = ["1", "2", "3"].iter().map(|s| ThreadId::from(*s)).collect();
    let map = ok_by_id(client_for(&server).mark_read_each(ids, 2).await);

    assert_eq!(map.len(), 3);
    assert!(map["1"]);
    assert!(map["2"]);
    assert!(
        !map["3"],
        "a 5xx should surface as Err without aborting the batch"
    );
}

#[tokio::test]
async fn mark_done_each_handles_success_and_failure() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/10"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/11"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_raw(r#"{"message":"Not Found"}"#, "application/json"),
        )
        .mount(&server)
        .await;

    let ids = vec![ThreadId::from("10"), ThreadId::from("11")];
    let map = ok_by_id(client_for(&server).mark_done_each(ids, 4).await);

    assert!(map["10"]);
    assert!(!map["11"]);
}

#[tokio::test]
async fn empty_input_returns_empty() {
    let server = MockServer::start().await;
    let results = client_for(&server)
        .mark_read_each(Vec::<ThreadId>::new(), 4)
        .await;
    assert!(results.is_empty());
}

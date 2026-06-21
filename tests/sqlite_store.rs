//! Integration tests for the SQLite-backed state store (issue #17).
#![cfg(feature = "sqlite-store")]

use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use octo_notify::app::{PollScope, SqliteStore, StateStore};
use octo_notify::{Auth, Client};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE1: &str = include_str!("fixtures/notifications_page1.json");

#[tokio::test]
async fn round_trip_persists_state() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");

    {
        let store = SqliteStore::open(&db).unwrap();
        store
            .set_last_modified(&PollScope::All, "Mon, 01 Jan 2024 00:00:00 GMT")
            .await
            .unwrap();
        let ts: DateTime<Utc> = "2024-05-01T10:00:00Z".parse().unwrap();
        store.record_seen(&"123456789".into(), ts).await.unwrap();
    }

    let store = SqliteStore::open(&db).unwrap();
    assert_eq!(
        store
            .last_modified(&PollScope::All)
            .await
            .unwrap()
            .as_deref(),
        Some("Mon, 01 Jan 2024 00:00:00 GMT")
    );
    assert!(store.seen(&"123456789".into()).await.unwrap().is_some());
    assert!(store.seen(&"absent".into()).await.unwrap().is_none());
}

#[tokio::test]
async fn prune_drops_old_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("prune.db");

    let store = SqliteStore::open(&db).unwrap();
    let old: DateTime<Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
    let new: DateTime<Utc> = "2030-01-01T00:00:00Z".parse().unwrap();
    store.record_seen(&"old".into(), old).await.unwrap();
    store.record_seen(&"new".into(), new).await.unwrap();
    store
        .prune("2025-01-01T00:00:00Z".parse().unwrap())
        .await
        .unwrap();

    let store = SqliteStore::open(&db).unwrap();
    assert!(store.seen(&"old".into()).await.unwrap().is_none());
    assert!(store.seen(&"new".into()).await.unwrap().is_some());
}

#[tokio::test]
async fn poller_honors_persisted_seen_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("poll.db");

    {
        let store = SqliteStore::open(&db).unwrap();
        let seen_at: DateTime<Utc> = "2030-01-01T00:00:00Z".parse().unwrap();
        store
            .record_seen(&"123456789".into(), seen_at)
            .await
            .unwrap();
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Last-Modified", "Mon, 01 Jan 2024 00:00:00 GMT")
                .set_body_raw(PAGE1, "application/json"),
        )
        .mount(&server)
        .await;

    let client = Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests")
        .build()
        .unwrap();

    let store = SqliteStore::open(&db).unwrap();
    let poller = client
        .poller()
        .min_interval(Duration::from_millis(20))
        .emit_existing_on_start(true)
        .store(store)
        .build();

    // 123456789 is already seen in the db; only 987654321 surfaces.
    let event = Box::pin(poller.stream())
        .next()
        .await
        .expect("a stream item")
        .expect("event ok");
    assert_eq!(event.notification().id.as_str(), "987654321");
}

//! Integration tests for the poller engine (M3), served by a mock GitHub (wiremock).

use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use octo_notify::app::{Event, MemoryStore, PollScope, StateStore};
use octo_notify::{Auth, Client, Reason};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE1: &str = include_str!("fixtures/notifications_page1.json");

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .auth(Auth::token("test-token"))
        .base_url(server.uri())
        .user_agent("octo-notify-tests")
        .build()
        .expect("client builds")
}

async fn mount_inbox(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Last-Modified", "Mon, 01 Jan 2024 00:00:00 GMT")
                .set_body_raw(PAGE1, "application/json"),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn first_tick_emits_all_in_ascending_order() {
    let server = MockServer::start().await;
    mount_inbox(&server).await;

    let poller = client_for(&server)
        .poller()
        .min_interval(Duration::from_millis(20))
        .emit_existing_on_start(true)
        .build();

    let events: Vec<Event> = Box::pin(poller.stream())
        .take(2)
        .map(|r| r.expect("event ok"))
        .collect()
        .await;

    // page 1 holds two items; ascending updated_at puts 05-01 before 05-02.
    assert_eq!(events.len(), 2);
    assert!(events[0].is_new());
    assert_eq!(events[0].notification().id.as_str(), "123456789");
    assert_eq!(events[1].notification().id.as_str(), "987654321");
}

#[tokio::test]
async fn already_seen_notifications_are_deduped() {
    let server = MockServer::start().await;
    mount_inbox(&server).await;

    // Pre-seed one of the two as already seen at a later time.
    let store = MemoryStore::new();
    let seen_at: DateTime<Utc> = "2030-01-01T00:00:00Z".parse().unwrap();
    store
        .record_seen(&"123456789".into(), seen_at)
        .await
        .unwrap();

    let poller = client_for(&server)
        .poller()
        .min_interval(Duration::from_millis(20))
        .emit_existing_on_start(true)
        .store(store)
        .build();

    let event = Box::pin(poller.stream())
        .next()
        .await
        .expect("a stream item")
        .expect("event ok");
    // The pre-seen 123456789 is skipped; only 987654321 surfaces.
    assert_eq!(event.notification().id.as_str(), "987654321");
}

#[tokio::test]
async fn reason_filter_keeps_only_matching() {
    let server = MockServer::start().await;
    mount_inbox(&server).await;

    let poller = client_for(&server)
        .poller()
        .min_interval(Duration::from_millis(20))
        .emit_existing_on_start(true)
        .reasons([Reason::Mention])
        .build();

    let event = Box::pin(poller.stream())
        .next()
        .await
        .expect("a stream item")
        .expect("event ok");
    assert_eq!(event.notification().reason, Reason::Mention);
    assert_eq!(event.notification().id.as_str(), "123456789");
}

#[tokio::test]
async fn default_first_tick_seeds_without_emitting() {
    let server = MockServer::start().await;
    mount_inbox(&server).await;

    // Default config: emit_existing_on_start = false. The first tick seeds the store and
    // subsequent ticks see the same (now-known) items, so nothing is ever emitted.
    let poller = client_for(&server)
        .poller()
        .min_interval(Duration::from_millis(20))
        .build();

    let mut stream = Box::pin(poller.stream());
    let result = tokio::time::timeout(Duration::from_millis(300), stream.next()).await;
    assert!(result.is_err(), "expected no events, timed out instead");
}

#[tokio::test]
async fn conditional_304_emits_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(header_exists("if-modified-since"))
        .respond_with(ResponseTemplate::new(304).insert_header("X-Poll-Interval", "0"))
        .mount(&server)
        .await;

    // A stored watermark makes the very first tick conditional -> 304 -> no events.
    let store = MemoryStore::new();
    store
        .set_last_modified(&PollScope::All, "Mon, 01 Jan 2024 00:00:00 GMT")
        .await
        .unwrap();

    let poller = client_for(&server)
        .poller()
        .min_interval(Duration::from_millis(20))
        .store(store)
        .build();

    let mut stream = Box::pin(poller.stream());
    let result = tokio::time::timeout(Duration::from_millis(300), stream.next()).await;
    assert!(result.is_err(), "304 should produce no events");
}

#[tokio::test]
async fn cancellation_ends_the_stream() {
    let server = MockServer::start().await;
    mount_inbox(&server).await;

    let token = CancellationToken::new();
    token.cancel();

    let poller = client_for(&server)
        .poller()
        .emit_existing_on_start(true)
        .cancellation(token)
        .build();

    let mut stream = Box::pin(poller.stream());
    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("stream resolves promptly");
    assert!(next.is_none(), "cancelled stream should end");
}

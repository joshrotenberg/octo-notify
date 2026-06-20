//! Integration tests for the refreshing token provider (issue #7).
#![cfg(feature = "token-refresh")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use octo_notify::{RefreshingToken, SecretString, TokenProvider};
use secrecy::ExposeSecret;

#[tokio::test]
async fn caches_fresh_token() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let provider = RefreshingToken::new(move || {
        let counter = counter.clone();
        async move {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            Ok((
                SecretString::new(format!("token-{n}").into_boxed_str()),
                Utc::now() + ChronoDuration::hours(1),
            ))
        }
    });

    let first = provider.token().await.unwrap();
    let second = provider.token().await.unwrap();
    assert_eq!(first.expose_secret(), "token-0");
    assert_eq!(
        second.expose_secret(),
        "token-0",
        "fresh token should be cached"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refreshes_stale_token() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let provider = RefreshingToken::new(move || {
        let counter = counter.clone();
        async move {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            // Already expired, so every call is considered stale.
            Ok((
                SecretString::new(format!("token-{n}").into_boxed_str()),
                Utc::now() - ChronoDuration::seconds(1),
            ))
        }
    });

    let _ = provider.token().await.unwrap();
    let second = provider.token().await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "stale token should refresh"
    );
    assert_eq!(second.expose_secret(), "token-1");
}

#[tokio::test]
async fn single_refresh_under_concurrency() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let provider = Arc::new(RefreshingToken::new(move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            // Widen the refresh window so concurrent callers overlap.
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok((
                SecretString::new("shared".to_owned().into_boxed_str()),
                Utc::now() + ChronoDuration::hours(1),
            ))
        }
    }));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let p = provider.clone();
        handles.push(tokio::spawn(async move {
            p.token().await.unwrap().expose_secret().to_owned()
        }));
    }
    for handle in handles {
        assert_eq!(handle.await.unwrap(), "shared");
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "concurrent callers should share one in-flight refresh"
    );
}

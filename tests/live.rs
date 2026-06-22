//! Live integration tests against the real GitHub API.
//!
//! Every test here is `#[ignore]`d, so `cargo test` and CI never run them. Run them on demand:
//!
//! ```sh
//! GITHUB_TOKEN=<token> cargo test --features cli --test live -- --ignored
//! ```
//!
//! The token must be a **classic PAT with the `notifications` scope** (fine-grained PATs and the
//! Actions `GITHUB_TOKEN` do not work for the notifications API). With the `gh` CLI:
//! `gh auth refresh -s notifications` then `GITHUB_TOKEN=$(gh auth token) ...`.
//!
//! Without a token the tests skip cleanly (they print a notice and return), so running with
//! `-- --ignored` is safe even unauthenticated.
//!
//! The subscription round-trip temporarily changes your watch state for a single repository
//! (`OCTONOTIFY_TEST_REPO`, default `octocat/Hello-World`) and restores it at the end.

use octo_notify::{Auth, Client, Listing};

/// Build a client from the environment, or `None` (skip) when no token is set.
fn live_client() -> Option<Client> {
    let auth = Auth::from_env().ok()?;
    Some(Client::new(auth).expect("client builds"))
}

#[cfg(feature = "cli")]
fn has_token() -> bool {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
}

/// The repository used for the subscription round-trip, as `(owner, name)`.
fn test_repo() -> (String, String) {
    let full =
        std::env::var("OCTONOTIFY_TEST_REPO").unwrap_or_else(|_| "octocat/Hello-World".to_string());
    let (owner, name) = full
        .split_once('/')
        .expect("OCTONOTIFY_TEST_REPO must be \"owner/name\"");
    (owner.to_string(), name.to_string())
}

macro_rules! skip_if_no_token {
    () => {
        match live_client() {
            Some(client) => client,
            None => {
                eprintln!(
                    "skipping live test: set GITHUB_TOKEN (classic PAT with `notifications` scope)"
                );
                return;
            }
        }
    };
}

/// Listing the inbox authenticates and round-trips a real response through the typed models.
#[tokio::test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` scope)"]
async fn live_list_inbox() {
    let client = skip_if_no_token!();
    let listing = client
        .notifications()
        .list()
        .per_page(10)
        .send()
        .await
        .expect("list notifications");
    if let Listing::Modified(page) = listing {
        // Fields are exercised by deserialization; just sanity-check what we can.
        for n in &page.items {
            assert!(!n.repository.full_name.is_empty());
        }
        eprintln!("inbox: {} notification(s) on this page", page.items.len());
    }
}

/// A second request carrying the prior `Last-Modified` should come back `304 Not Modified`.
#[tokio::test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` scope)"]
async fn live_conditional_request_not_modified() {
    let client = skip_if_no_token!();
    let first = client
        .notifications()
        .list()
        .send()
        .await
        .expect("first list");
    let Some(last_modified) = first.last_modified().map(str::to_owned) else {
        eprintln!("skipping 304 assertion: server returned no Last-Modified header");
        return;
    };
    let second = client
        .notifications()
        .list()
        .if_modified_since(last_modified)
        .send()
        .await
        .expect("conditional list");
    // Back-to-back this is reliably 304; it can rarely be 200 if new activity lands in between.
    assert!(
        !second.is_modified(),
        "expected 304 Not Modified for an unchanged inbox (new activity may have raced in)"
    );
}

/// Subscribe to a repository, read it back, delete the subscription, and restore prior state.
#[tokio::test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` + `repo` scope)"]
async fn live_subscription_round_trip() {
    let client = skip_if_no_token!();
    let (owner, name) = test_repo();
    let repo = client.repo(owner.as_str(), name.as_str());

    // Record the starting state so we can restore it (404 means "not subscribed").
    let original = match repo.subscription().await {
        Ok(sub) => Some((sub.subscribed, sub.ignored)),
        Err(octo_notify::Error::Api { status, .. }) if status.as_u16() == 404 => None,
        Err(e) => panic!("unexpected error reading subscription: {e}"),
    };

    let subscribed = repo.subscribe().await.expect("subscribe");
    assert!(subscribed.subscribed, "subscribe() should set subscribed");
    let got = repo.subscription().await.expect("get after subscribe");
    assert!(got.subscribed, "subscription() should report subscribed");

    repo.delete_subscription()
        .await
        .expect("delete subscription");
    match repo.subscription().await {
        Err(octo_notify::Error::Api { status, .. }) => {
            assert_eq!(
                status.as_u16(),
                404,
                "subscription should be gone after delete"
            );
        }
        Ok(sub) => panic!(
            "expected 404 after delete, got subscribed={} ignored={}",
            sub.subscribed, sub.ignored
        ),
        Err(e) => panic!("unexpected error after delete: {e}"),
    }

    // Restore whatever state the repo was in before the test.
    if let Some((subscribed, ignored)) = original {
        repo.set_subscription(subscribed, ignored)
            .await
            .expect("restore original subscription");
    }
}

/// Listing your watched repositories authenticates and paginates against the real endpoint.
#[tokio::test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` scope)"]
async fn live_list_subscriptions() {
    let client = skip_if_no_token!();
    let repos = client
        .subscriptions()
        .all()
        .await
        .expect("list subscriptions");
    eprintln!(
        "watching {} repositor{}",
        repos.len(),
        if repos.len() == 1 { "y" } else { "ies" }
    );
}

/// The `subscriptions` subcommand runs end-to-end through the built binary.
#[cfg(feature = "cli")]
#[test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` scope)"]
fn live_cli_subscriptions() {
    if !has_token() {
        eprintln!("skipping live CLI test: set GITHUB_TOKEN");
        return;
    }
    // The token is inherited from this process's environment.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_octo-notify"))
        .arg("subscriptions")
        .output()
        .expect("spawn octo-notify");
    assert!(
        output.status.success(),
        "`octo-notify subscriptions` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The `inbox` subcommand runs end-to-end through the built binary.
#[cfg(feature = "cli")]
#[test]
#[ignore = "live: requires GITHUB_TOKEN (classic PAT with `notifications` scope)"]
fn live_cli_inbox() {
    if !has_token() {
        eprintln!("skipping live CLI test: set GITHUB_TOKEN");
        return;
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_octo-notify"))
        .args(["inbox", "--per-page", "5"])
        .output()
        .expect("spawn octo-notify");
    assert!(
        output.status.success(),
        "`octo-notify inbox` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

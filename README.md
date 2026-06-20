# octo-notify

[![CI](https://github.com/joshrotenberg/octo-notify/actions/workflows/ci.yml/badge.svg)](https://github.com/joshrotenberg/octo-notify/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/octo-notify.svg)](https://crates.io/crates/octo-notify)
[![docs.rs](https://img.shields.io/docsrs/octo-notify)](https://docs.rs/octo-notify)
[![license](https://img.shields.io/crates/l/octo-notify.svg)](#license)

A Rust library focused entirely on the **GitHub Notifications API**: complete, typed coverage
of every endpoint, plus an application engine (poller, async event stream, and state) for
building things on top of the notifications inbox.

> Status: pre-0.1, under active development. The API may change before the first release.

## Why this exists

GitHub provides no push/webhook for the notifications inbox, so anything that reacts to
notifications has to poll, correctly: conditional requests, `X-Poll-Interval` obedience,
deduplication, and resilience. That logic gets re-implemented in every notification CLI and
bot. octo-notify is that logic as a reusable crate.

It does not compete with general clients like [`octocrab`](https://crates.io/crates/octocrab).
octocrab covers the whole GitHub API but exposes notifications as thin one-shot endpoint
wrappers, with no poller, no conditional-request handling, no dedupe, and `reason`/`type` as
plain strings. octo-notify is the focused, application-shaped layer octocrab deliberately
doesn't have. You can use both side by side.

## Quick start

```toml
[dependencies]
octo-notify = "0.1"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
```

List your unread notifications:

```rust
use octo_notify::{Auth, Client, Listing, Reason};

#[tokio::main]
async fn main() -> octo_notify::Result<()> {
    let client = Client::new(Auth::from_env()?)?; // reads GITHUB_TOKEN / GH_TOKEN
    let listing = client.notifications().list().participating(true).send().await?;

    if let Listing::Modified(page) = listing {
        for n in &page.items {
            println!("[{}] {} - {}", n.reason, n.repository.full_name, n.subject.title);
        }
    }
    Ok(())
}
```

Watch the inbox as a live stream of events:

```rust
use futures::StreamExt;
use octo_notify::app::Event;
use octo_notify::{Auth, Client};

#[tokio::main]
async fn main() -> octo_notify::Result<()> {
    let client = Client::new(Auth::from_env()?)?;
    let mut events = Box::pin(client.poller().build().stream());

    while let Some(event) = events.next().await {
        match event? {
            Event::New(n) => println!("new: {}", n.subject.title),
            Event::Updated(n) => println!("updated: {}", n.subject.title),
        }
    }
    Ok(())
}
```

Two runnable examples ship in [`examples/`](examples):

```sh
GITHUB_TOKEN=$(gh auth token) cargo run --example inbox -- --all
GITHUB_TOKEN=$(gh auth token) cargo run --example watch -- --all --show-existing
```

## Design

Three layers, each usable without the one above it:

1. **Typed client** - every Notifications endpoint (inbox + repo list/mark, thread
   get/read/done, the three subscription operations), conditional requests
   (`If-Modified-Since` / `304`), rate-limit accounting, pagination (`all()` / `stream()`),
   and forward-compatible `Reason` / `SubjectType` enums that never fail on a value GitHub
   adds later.
2. **App engine** - a `Poller` that yields an async `Stream` of `New` / `Updated` events,
   with client-side filters (reason, subject type, repo, predicate).
3. **State** - a `StateStore` trait (with an in-memory implementation) so a long-running
   poller dedupes across restarts.

The poller is designed for long-running processes: transient errors (network, 5xx,
rate limits) are retried with backoff while fatal errors end the stream; events are emitted
in ascending `updated_at` with at-least-once delivery; the conditional check applies to the
first page, then all pages are fetched before classification; and shutdown is cooperative via
a `CancellationToken`.

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `rustls` | yes | TLS via rustls |
| `native-tls` | no | TLS via the platform's native stack |
| `stream` | yes | The poller engine and `stream()` pagination (Layer 2/3) |

Disable defaults for a minimal Layer-1 client: `default-features = false, features = ["rustls"]`.

## Authentication

The notifications inbox is user-scoped. A classic PAT needs the `notifications` scope (or
`repo` to also read issue/commit subjects); a fine-grained PAT needs the read-only
"Notifications" account permission. Tokens are supplied through a `TokenProvider`, so a
GitHub App user-to-server token that expires can be refreshed without changing call sites.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.

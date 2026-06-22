# octo-notify

[![CI](https://github.com/joshrotenberg/octo-notify/actions/workflows/ci.yml/badge.svg)](https://github.com/joshrotenberg/octo-notify/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/octo-notify.svg)](https://crates.io/crates/octo-notify)
[![docs.rs](https://img.shields.io/docsrs/octo-notify)](https://docs.rs/octo-notify)
[![license](https://img.shields.io/crates/l/octo-notify.svg)](#license)

A Rust library focused entirely on the **GitHub Notifications API**: complete, typed coverage
of every endpoint, plus an application engine (poller, async event stream, and state) for
building things on top of the notifications inbox.

> Status: pre-0.1, under active development. The API may change before the first release.

## Scope

octo-notify covers the GitHub Notifications REST API and nothing else. GitHub has no webhook
for the notifications inbox, so consuming it requires polling with conditional requests,
`X-Poll-Interval` handling, deduplication, and retry/backoff. This crate provides those as a
library rather than leaving them to each caller.

### vs. octocrab

[`octocrab`](https://crates.io/crates/octocrab) is a general GitHub client; its notifications
support is a set of one-shot endpoint wrappers. octo-notify covers the same endpoints and adds
what a long-running consumer needs:

- a `Poller` that yields a `Stream` of `New`/`Updated` events
- conditional requests (`If-Modified-Since` / `304`) and `X-Poll-Interval` handling
- deduplication via a pluggable `StateStore`
- `Reason` and `SubjectType` as typed enums with forward-compatible `Unknown` variants
  (octocrab models these as plain strings)
- the `DELETE /notifications/threads/{id}` "mark as done" endpoint (octocrab omits it)

The two compose: octocrab for the rest of the API, octo-notify for the inbox.

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

## CLI

A command-line tool ships behind the `cli` feature:

```sh
cargo install octo-notify --features cli

GITHUB_TOKEN=$(gh auth token) octo-notify inbox --all
GITHUB_TOKEN=$(gh auth token) octo-notify watch --state ~/.cache/octo-notify.json
```

`watch --state <PATH>` persists dedupe state so restarts resume without re-firing.

### Dispatch

`octo-notify dispatch` runs a command per notification, driven by a TOML rules file (see
[`dispatch.example.toml`](dispatch.example.toml) for a fully commented reference):

```toml
match = "first"   # or "all"

[[rule]]
reason = "mention"
run = "notify-send {repo} {title}"

[[rule]]
subject_type = "Issue"
run = "my-handler {url}"
mark = "read"     # mark the thread read on a zero exit (optional)
```

```sh
GITHUB_TOKEN=$(gh auth token) octo-notify dispatch --config dispatch.toml --state state.json
```

Matchers (`reason`/`subject_type`/`repo`) are ANDed; omitted ones match anything. `run` gets
`{repo} {thread_id} {title} {url} {reason} {type}` substituted (also exported as `OCTO_*` env
vars). The command can be anything - a script, `notify-send`, `curl`, or a task runner.

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
| `file-store` | no | `JsonFileStore`, a file-backed `StateStore` for cross-restart dedupe |
| `sqlite-store` | no | `SqliteStore`, a SQLite-backed `StateStore` |
| `token-refresh` | no | `RefreshingToken`, a caching `TokenProvider` for expiring credentials |
| `retry` | no | `RetryPolicy`, auto-retry for rate-limited calls |
| `tracing` | no | Structured `tracing` instrumentation of requests and the poller |
| `cli` | no | The `octo-notify` command-line binary |

Disable defaults for a minimal Layer-1 client: `default-features = false, features = ["rustls"]`.

## Authentication

The notifications inbox is user-scoped. A classic PAT needs the `notifications` scope (or
`repo` to also read issue/commit subjects); a fine-grained PAT needs the read-only
"Notifications" account permission. Tokens are supplied through a `TokenProvider`, so a
GitHub App user-to-server token that expires can be refreshed without changing call sites.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.

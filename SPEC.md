# octo-notify — Design Spec

> A Rust library focused 100% on the GitHub Notifications API: complete, typed coverage of
> every endpoint, plus an application engine (poller + async stream + state) for building
> things on top of the notifications inbox.

Status: **draft / pre-0.1**. This document is the design contract; code follows it.

---

## 1. Mission and scope

octo-notify does one thing well: it is the definitive Rust crate for working with a GitHub
user's **notifications inbox**. It is *not* a general GitHub client — it deliberately covers
only the notifications surface, and covers it completely.

No existing crate fills this niche: `notify` is filesystem-watching, `notify-rust` is desktop
popups, and general clients like `octocrab` treat notifications as a thin afterthought with no
poller, dedupe, or conditional-request handling. That gap — a focused, correct notifications
engine — is the reason this crate exists.

**In scope**

- Full, typed coverage of all Notifications REST endpoints (user-level, repo-level, thread,
  subscription).
- The mechanics that make notifications usable as a live feed: conditional requests
  (`If-Modified-Since` / `304`), `X-Poll-Interval` obedience, pagination, rate-limit handling.
- An application engine: a poller that emits an async `Stream` of notification events,
  client-side filtering, bulk actions, and a pluggable state store so a long-running app
  doesn't re-notify across restarts.

**Out of scope** (use a general client like octocrab alongside us)

- Issues/PRs/repos/anything that isn't the notifications surface. The one concession: the
  `subject.url` of a notification points at an issue/PR/commit/etc., and we expose helpers to
  *fetch the raw JSON* of that URL, but we do not model those resources.
- GraphQL — notifications are REST-only at GitHub.
- A UI/TUI. (An `examples/` TUI may exist as a demo, never as a crate dependency.)

---

## 2. Design principles

1. **Forward-compatible enums.** GitHub adds notification `reason`s and subject `type`s over
   time. Every such enum has an `Unknown(String)` variant and never fails deserialization on
   an unseen value. A notifications library that breaks when GitHub ships a new reason is
   broken by design.
2. **The polling mechanics are the product.** Conditional requests, `X-Poll-Interval`, and
   rate-limit accounting aren't an afterthought layered on a dumb client — they are first-class
   and correct by default. A 304 must cost zero rate-limit budget and the library must make
   that automatic.
3. **Layered, each layer usable alone.** You can use Layer 1 (the typed client) without ever
   touching the poller. You can use the poller without the file store. No layer forces the one
   above it.
4. **Secrets don't leak.** Tokens are wrapped so they never appear in `Debug` output or logs.
5. **Async-first, runtime-light.** Tokio + reqwest. No blocking facade in 0.1 (revisit if asked).
6. **Typed errors, no panics on bad input.** Every fallible path returns `Result<_, Error>`.

---

## 3. Architecture (three layers)

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3 — State (optional)                                    │
│   StateStore trait · MemoryStore · JsonFileStore · (Sqlite)   │
│   Persists Last-Modified + seen-thread set for cross-restart  │
│   dedupe.                                                      │
├─────────────────────────────────────────────────────────────┤
│ Layer 2 — App engine                                          │
│   Poller → Stream<Item = Result<Event>>                       │
│   PollConfig (interval floor, scope, all/participating)       │
│   Filters (reason / subject-type / repo / custom predicate)   │
│   Bulk actions (mark many read/done with bounded concurrency) │
├─────────────────────────────────────────────────────────────┤
│ Layer 1 — Typed client (full API coverage)                    │
│   Client + ClientBuilder · Auth · Error · RateLimit           │
│   Endpoint handlers · request builders · Page/pagination      │
│   Conditional requests, header parsing, models                │
├─────────────────────────────────────────────────────────────┤
│   reqwest · tokio · serde / serde_json · chrono · url         │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. Dependencies and feature flags

| Crate | Why |
|---|---|
| `reqwest` (rustls default) | HTTP transport |
| `tokio` | async runtime (caller provides the runtime; we just `await`) |
| `serde`, `serde_json` | (de)serialization |
| `chrono` | `DateTime<Utc>` for timestamps |
| `url` | typed URLs |
| `http` | status codes / header names |
| `thiserror` | error derive |
| `async-trait` | object-safe `StateStore` (for `Box<dyn StateStore>`) |
| `futures` / `tokio-stream` | the `Stream` adapter (feature-gated) |
| `async-stream` | implements the `Poller::stream()` generator loop (feature-gated) |
| `secrecy` | wrap tokens so they don't leak in logs/Debug |
| `tracing` (optional) | structured instrumentation |

**Features**

```toml
default       = ["rustls", "stream", "memory-store"]
rustls        = ["reqwest/rustls-tls"]
native-tls    = ["reqwest/native-tls"]
stream        = ["dep:futures", "dep:tokio-stream", "dep:async-stream"]  # Layer 2 poller/stream
memory-store  = []                                     # in-process state (Layer 3)
file-store    = ["dep:serde_json"]                     # JSON-file state store
sqlite-store  = ["dep:rusqlite"]                        # future
tracing       = ["dep:tracing"]
```

Layer 1 alone = no `stream` feature needed.

---

## 5. Layer 1 — Typed client

### 5.1 Client and auth

```rust
pub struct Client { /* reqwest::Client, base_url, auth, default poll floor */ }

impl Client {
    pub fn builder() -> ClientBuilder;
    pub fn new(auth: Auth) -> Result<Self>;          // sensible defaults
}

pub struct ClientBuilder { /* ... */ }
impl ClientBuilder {
    pub fn auth(self, auth: Auth) -> Self;
    pub fn user_agent(self, ua: impl Into<String>) -> Self;     // required by GitHub
    pub fn base_url(self, url: Url) -> Self;                     // GHES: https://ghe.host/api/v3
    pub fn http_client(self, client: reqwest::Client) -> Self;  // BYO reqwest (proxy/timeouts)
    pub fn retry(self, policy: RetryPolicy) -> Self;            // rate-limit/backoff behavior
    pub fn build(self) -> Result<Client>;
}

pub enum Auth {
    /// Classic PAT or user-to-server OAuth token. Sent as `Authorization: Bearer <token>`.
    Token(SecretString),
}
impl Auth {
    pub fn token(t: impl Into<String>) -> Self;
    pub fn from_env() -> Result<Self>;   // GITHUB_TOKEN / GH_TOKEN
}
```

**Request defaults.** The base URL defaults to `https://api.github.com` (the REST API host —
*not* `https://github.com`, a common mistake). Every request sends `User-Agent` (required by
GitHub), `Accept: application/vnd.github+json`, and pins `X-GitHub-Api-Version: 2022-11-28` so a
future default API version can't silently change response shapes. `base_url` overrides the host
for GHES (`https://ghe.host/api/v3`).

**Auth note.** The notifications inbox is inherently *user-scoped*. A classic PAT needs the
`notifications` scope (or `repo` to also read issue/commit subjects); a fine-grained PAT needs
the read-only "Notifications" account permission. There is no pure app-to-server notifications
inbox, so we model auth as a single bearer token and document the scope requirements rather
than enumerating credential types.

### 5.2 Endpoint coverage — every endpoint mapped

Handlers are entry points; request builders carry optional params and `.send()`.

| REST endpoint | Library call |
|---|---|
| `GET /notifications` | `client.notifications().list()….send()` / `.stream()` |
| `PUT /notifications` | `client.notifications().mark_all_read()….send()` |
| `GET /notifications/threads/{id}` | `client.thread(id).get().await` |
| `PATCH /notifications/threads/{id}` | `client.thread(id).mark_read().await` |
| `DELETE /notifications/threads/{id}` | `client.thread(id).mark_done().await` |
| `GET /notifications/threads/{id}/subscription` | `client.thread(id).subscription().await` |
| `PUT /notifications/threads/{id}/subscription` | `client.thread(id).set_subscription(ignored).await` |
| `DELETE /notifications/threads/{id}/subscription` | `client.thread(id).delete_subscription().await` |
| `GET /repos/{o}/{r}/notifications` | `client.repo(o, r).notifications().list()….send()` / `.stream()` |
| `PUT /repos/{o}/{r}/notifications` | `client.repo(o, r).notifications().mark_all_read()….send()` |

That is the complete Notifications API. List-request builders share the param set:

```rust
let page = client.notifications().list()
    .all(false)              // include already-read
    .participating(true)     // only where you're a participant/@mentioned
    .since(ts)               // updated after
    .before(ts)              // updated before
    .per_page(50)            // global max 50; repo-scoped max 100
    .page(1)
    .send().await?;          // -> Page<Notification>

// mark-all-read body params:
client.notifications().mark_all_read().last_read_at(ts).send().await?;
```

### 5.3 Models

```rust
pub struct Notification {
    pub id: ThreadId,                       // newtype over String (API returns string)
    pub repository: MinimalRepository,
    pub subject: Subject,
    pub reason: Reason,
    pub unread: bool,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub url: Url,
    pub subscription_url: Url,
}

pub struct Subject {
    pub title: String,
    pub url: Option<Url>,                   // null for some subject types
    pub latest_comment_url: Option<Url>,
    pub kind: SubjectType,                  // serde(rename = "type")
}

// Forward-compatible: unknown values deserialize to Unknown(_) instead of erroring.
pub enum SubjectType {
    Issue, PullRequest, Commit, Release, Discussion,
    RepositoryVulnerabilityAlert, CheckSuite, RepositoryInvitation,
    Unknown(String),
}

pub enum Reason {
    Assign, Author, Comment, CiActivity, Invitation, Manual, Mention,
    ReviewRequested, SecurityAlert, StateChange, Subscribed, TeamMention,
    ApprovalRequested, MemberFeatureRequested, SecurityAdvisoryCredit, YourActivity,
    Unknown(String),
}

pub struct ThreadSubscription {
    pub subscribed: bool,
    pub ignored: bool,
    pub reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub url: Url,
    pub thread_url: Option<Url>,
    pub repository_url: Option<Url>,
}

// Only the fields the notifications payload actually carries; not a full Repository.
pub struct MinimalRepository {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: SimpleUser,
    pub private: bool,
    pub html_url: Url,
    pub fork: bool,
}
```

Subject helpers (the one bridge to general resources):

```rust
impl Subject {
    pub fn issue_number(&self) -> Option<u64>;   // parse from url for Issue/PullRequest
    pub fn is_pull_request(&self) -> bool;
}
impl Client {
    /// Fetch the raw JSON the subject points at (issue/PR/commit/release/...).
    /// We don't model the result — return serde_json::Value so callers map it themselves.
    pub async fn fetch_subject(&self, n: &Notification) -> Result<serde_json::Value>;
}
```

### 5.4 Pagination

```rust
pub struct Page<T> {
    pub items: Vec<T>,
    pub poll_interval: Option<Duration>,    // X-Poll-Interval
    pub last_modified: Option<String>,      // feed back as If-Modified-Since
    pub rate_limit: RateLimit,
    next: Option<Url>,                      // parsed from Link header
}
impl<T> Page<T> {
    pub fn has_next(&self) -> bool;
}

// Convenience: auto-follow Link rel="next" as a Stream (feature = "stream").
impl ListNotificationsBuilder {
    pub fn stream(self) -> impl Stream<Item = Result<Notification>>;  // across all pages
    pub async fn all(self) -> Result<Vec<Notification>>;              // collect everything
}
```

### 5.5 Conditional requests, rate limits, errors

```rust
pub struct RateLimit {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset_at: Option<DateTime<Utc>>,
    pub used: Option<u32>,
}
pub enum RateLimitKind { Primary, Secondary }

pub enum Error {
    Http(reqwest::Error),
    Api { status: StatusCode, message: String, doc_url: Option<String>, errors: Vec<ApiFieldError> },
    RateLimited { kind: RateLimitKind, retry_after: Option<Duration>, reset_at: Option<DateTime<Utc>> },
    Unauthorized,
    Deserialize { source: serde_json::Error, body: String },   // body kept for debugging
    InvalidBaseUrl,
}
pub type Result<T> = std::result::Result<T, Error>;
```

- **304 handling.** Any request may carry a caller-supplied `If-Modified-Since`. A `304`
  surfaces as `Ok(NotModified)` (a small enum / `Option`-like wrapper on conditional calls),
  *not* an error, and reports `rate_limit` unchanged. This is what makes free polling free.
- **Retry policy.** `RetryPolicy` controls auto-retry on primary rate-limit exhaustion (sleep
  until `reset_at`) and secondary limits (`Retry-After`). Default: retry secondary limits with
  the server's `Retry-After`; surface primary exhaustion as `RateLimited` for the caller to
  decide. The poller (Layer 2) has its own interval logic on top of this.

---

## 6. Layer 2 — App engine

### 6.1 Poller and event stream

GitHub provides **no push/webhook for the notifications inbox** — events feed it server-side, but
there is no subscription that pushes the inbox to you. Polling is the only mechanism, which is
precisely why a correct poller (conditional requests, interval obedience, dedupe, resilience) is
the core of this crate rather than a convenience wrapper.

```rust
pub struct Poller { /* client, config, store, filters */ }

pub struct PollConfig {
    pub scope: PollScope,                 // All inbox | Repo { owner, name }
    pub include_read: bool,               // maps to `all`
    pub participating_only: bool,         // maps to `participating`
    pub min_interval: Duration,           // floor; never poll faster (default 60s)
    pub respect_server_interval: bool,    // honor X-Poll-Interval (default true)
    pub emit_existing_on_start: bool,     // emit current inbox once, or only deltas (default false)
}

pub enum PollScope { All, Repo { owner: String, name: String } }

pub enum Event {
    New(Notification),       // not previously seen
    Updated(Notification),   // seen before; updated_at advanced (e.g. new comment, reason change)
}

impl Client {
    pub fn poller(&self) -> PollerBuilder;   // -> set scope/config/filters/store
}
impl Poller {
    /// The core abstraction. Yields events until the stream is dropped, a cancellation
    /// token fires, or a fatal error ends it (see §6.4 for the resilience contract).
    pub fn stream(self) -> impl Stream<Item = Result<Event>>;
}
```

**Poll loop (per tick):**

1. Read `last_modified` for the scope from the state store; send the list request with
   `If-Modified-Since`.
2. `304` → no changes. Sleep `max(server X-Poll-Interval, min_interval)`; loop.
3. `200` → for each notification, consult the store: unseen → `Event::New`; seen but
   `updated_at` advanced → `Event::Updated`; unchanged → skip. Apply filters. Persist new
   `last_modified` and per-thread `updated_at`. Emit surviving events. Sleep; loop.

The loop obeys `X-Poll-Interval` (clamped up to `min_interval`) so it is correct under high
server load by default, and 304s keep it within rate budget indefinitely.

### 6.2 Filters

Applied client-side after fetch (the API only filters by `all`/`participating`/`since`).

```rust
pub enum Filter {
    Reason(Reason),
    SubjectType(SubjectType),
    Repo { owner: String, name: String },
    Predicate(Arc<dyn Fn(&Notification) -> bool + Send + Sync>),  // escape hatch
}
// Builder sugar: .filter_reason(Reason::Mention).filter_repo("o","r").filter(|n| ...)
```

### 6.3 Bulk actions

```rust
impl Client {
    /// Mark many threads read/done concurrently with a bounded worker pool.
    pub async fn mark_read_each(&self, ids: impl IntoIterator<Item = ThreadId>, concurrency: usize)
        -> Vec<(ThreadId, Result<()>)>;
    pub async fn mark_done_each(&self, ids: impl IntoIterator<Item = ThreadId>, concurrency: usize)
        -> Vec<(ThreadId, Result<()>)>;
}
```

Per-thread results are returned individually so one failure doesn't sink the batch.

### 6.4 Poller robustness contract

A library that fronts a *days-long* poll loop lives or dies on its failure behavior. This is the
contract.

**Error policy — transient vs fatal.** A naive `Stream` that ends on the first network blip is
useless for a background watcher. The poller classifies each poll error:

- **Transient** (timeouts, 5xx, secondary rate limit, primary-limit exhaustion): do *not* end the
  stream. Back off and retry the tick. These surface as `Event`-stream items only if the caller
  opts into seeing them (`ErrorPolicy::emit_transient`); by default they're retried silently and
  logged via `tracing`.
- **Fatal** (`401 Unauthorized`, `InvalidBaseUrl`, a deserialize error that recurs): yield the
  `Err` once and end the stream — retrying can't help.

```rust
pub struct ErrorPolicy {
    pub max_consecutive_failures: Option<u32>, // None = retry forever (default)
    pub backoff: Backoff,                      // base, max, multiplier, jitter
    pub emit_transient: bool,                  // default false: retry silently
}
```

**Backoff.** Transient retries use exponential backoff with jitter, capped, and *independent* of
the normal poll cadence. On recovery, the loop returns to the `X-Poll-Interval`/`min_interval`
rhythm.

**Delivery semantics — at-least-once.** Within a tick, the poller emits events in **ascending
`updated_at`** order (oldest first — natural for a feed), then commits per-thread `seen` records,
and only after the whole tick is durably recorded commits the new `last_modified`. A crash
mid-tick re-fetches (the watermark wasn't advanced) and re-emits — so a consumer may see a given
notification more than once but never miss one. For notifications, double-notifying beats
dropping. Consumers that need exactly-once dedupe downstream on `(id, updated_at)`.

**Pagination vs conditional requests.** The two compose carefully: the conditional `If-Modified-
Since` and the `X-Poll-Interval` / `Last-Modified` capture apply to the **first page** of a tick.
On a `200`, the poller records page 1's `Last-Modified`, then follows `Link rel="next"`
*unconditionally* to assemble the full current set before classifying — so a multi-page inbox is
never half-read, and the next tick's 304 check still works off page 1.

**Shutdown.** Dropping the stream stops the loop at the next `await` point. For cooperative
shutdown without dropping (e.g. flush state first), the builder accepts a
`tokio_util::sync::CancellationToken`; when it fires, the current tick finishes, state is
committed, and the stream ends cleanly.

**Eventual consistency of mutations.** `mark_all_read` may return `202 Accepted` (processed
asynchronously) for large inboxes. After a bulk mark, the *next* poll can still briefly show the
just-marked items. The poller tolerates this naturally (already-seen → skipped); callers driving
the API directly should not assume read-state is immediately reflected.

---

## 7. Layer 3 — State store

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn last_modified(&self, scope: &PollScope) -> Result<Option<String>>;
    async fn set_last_modified(&self, scope: &PollScope, value: &str) -> Result<()>;
    async fn seen(&self, id: &ThreadId) -> Result<Option<DateTime<Utc>>>;  // last seen updated_at
    async fn record_seen(&self, id: &ThreadId, updated_at: DateTime<Utc>) -> Result<()>;
    async fn prune(&self, older_than: DateTime<Utc>) -> Result<()>;        // GC old seen records
}
```

| Impl | Feature | Use |
|---|---|---|
| `MemoryStore` | `memory-store` (default) | single-process, resets on restart |
| `JsonFileStore` | `file-store` | small apps; survives restarts; atomic write-rename |
| `SqliteStore` | `sqlite-store` (future) | many threads / concurrent processes |

Default poller uses `MemoryStore`. Swap via `client.poller().store(JsonFileStore::open(path)?)`.

---

## 8. Module layout

```
src/
  lib.rs            crate docs + re-exports + prelude
  client.rs         Client, ClientBuilder
  auth.rs           Auth, secret wrapping
  error.rs          Error, Result
  http.rs           request execution, header parsing, conditional-request glue
  rate_limit.rs     RateLimit, RateLimitKind, RetryPolicy
  pagination.rs     Page<T>, Link parsing, stream adapter
  models/           notification, subject, reason, subscription, repository, user
  endpoints/        notifications, threads, repo (handlers + request builders)
  app/
    poller.rs       Poller, PollerBuilder, PollConfig, Event   (feature = "stream")
    filter.rs       Filter
    store/          mod (trait) + memory + file
  prelude.rs        common re-exports for `use octo_notify::prelude::*;`
```

---

## 9. Usage examples

**List unread mentions**

```rust
let client = Client::new(Auth::from_env()?)?;
let page = client.notifications().list().participating(true).send().await?;
for n in page.items.iter().filter(|n| matches!(n.reason, Reason::Mention)) {
    println!("{} — {}", n.repository.full_name, n.subject.title);
}
```

**Watch the inbox as a stream (survives restarts)**

```rust
use futures::StreamExt;

let client = Client::new(Auth::from_env()?)?;
let mut events = client.poller()
    .scope(PollScope::All)
    .min_interval(Duration::from_secs(60))
    .store(JsonFileStore::open("~/.cache/octo-notify.json")?)
    .filter_reason(Reason::ReviewRequested)
    .build()
    .stream();

while let Some(event) = events.next().await {
    match event? {
        Event::New(n)     => notify_desktop(&n),
        Event::Updated(n) => log::debug!("updated: {}", n.subject.title),
    }
}
```

**Triage: mark all CI notifications done**

```rust
let page = client.notifications().list().all(true).per_page(50).send().await?;
let ci: Vec<_> = page.items.iter()
    .filter(|n| matches!(n.reason, Reason::CiActivity))
    .map(|n| n.id.clone()).collect();
let results = client.mark_done_each(ci, 8).await;
```

---

## 10. Testing strategy

- **Serde fixtures.** Capture real API payloads under `tests/fixtures/`; round-trip every
  model. Include payloads with *unknown* reasons/subject-types to lock in the forward-compat
  guarantee.
- **`wiremock` integration.** Mock GitHub at the HTTP layer: verify `If-Modified-Since` is sent,
  `304` keeps rate budget, `Link` pagination is followed, `X-Poll-Interval` is honored, and the
  poller dedupes via the store.
- **Poller unit tests.** Drive the loop with a fake clock + fake store + scripted responses;
  assert New vs Updated vs skip classification.
- **Live smoke test.** `#[ignore]`-gated, runs only with a real `GITHUB_TOKEN`.

---

## 11. Roadmap

| Milestone | Contents |
|---|---|
| M1 — transport ✅ | `Client`/`Auth`/`Error` + `TokenProvider`, `GET /notifications`, conditional requests (304), header + rate-limit parsing, forward-compat models, wiremock tests, `inbox` example. Done; verified live. |
| M2 — full coverage ✅ | All 13 endpoints (inbox + repo list/mark, thread get/read/done, 3 subscription ops), shared request layer, `all()` + `stream()` pagination, `ThreadSubscription` model. Done. |
| M3 — engine | `Poller` + `Stream`, filters, `MemoryStore`, conditional-request loop, robustness contract (§6.4): error policy, backoff, ascending emit, cancellation |
| M4 — persistence + bulk | `JsonFileStore`, bulk mark-read/done, `RetryPolicy`, `TokenProvider` seam, opt-in `prune_after` |
| M5 — polish | docs, `examples/`, CI matrix, MSRV pin, `0.1.0` to crates.io |
| later | `SqliteStore`, optional sync facade, GHES test coverage, TUI demo |

---

## 12. Decisions

Resolved for 0.1:

1. **`fetch_subject` returns `serde_json::Value`.** We stay strictly in the notifications lane;
   callers map the subject payload themselves. A typed `Subject::resolve()` can be added later
   without a breaking change.
2. **`Event::Updated` classifies on `updated_at` advance only.** No `reason`/`subject` diffing in
   0.1. The notification carries the new `reason`, so a consumer that cares can compare. Diff-based
   classification is a non-breaking later addition.
3. **Async-only.** No blocking facade in 0.1. Revisit if a sync consumer appears.
4. **MSRV = 1.85** (the edition-2024 floor). Pinned in `Cargo.toml` and tested as a CI matrix row.
5. **License: dual MIT OR Apache-2.0** (Rust ecosystem convention).
6. **Naming:** keep `octo-notify` crate / `octo_notify` lib (octocrab-adjacent namespace).
7. **"Mark as done" is one-way.** The API has no un-done; documented prominently on `mark_done`.

## 13. Still open

These don't block M1 but need a call before the milestone that touches them:

1. **Token refresh (M4+).** `Auth::Token` is a static bearer. Classic PATs are long-lived, but
   GitHub App *user-to-server* tokens expire (~8h) — a poller running for days will see 401s. To
   support that, `Auth` should grow a `TokenProvider` trait (an async `fn token()` the client
   calls per request / on 401) without breaking the static-token path. Decide whether to design
   the seam in now or defer. Leaning: add the trait in M4, keep `Auth::token()` as the trivial impl.
2. **`prune` policy.** `StateStore::prune` exists but nothing calls it. Should the poller auto-prune
   seen-records older than N days, or leave GC entirely to the caller? Leaning: opt-in
   `PollConfig::prune_after: Option<Duration>`, default `None`.
3. **`Updated` for read-state changes.** If a thread is marked read elsewhere (web UI), should the
   poller emit anything? Currently no (we only track `updated_at`). A `Read`/`Done` event would
   need extra state and is probably out of scope — confirm.

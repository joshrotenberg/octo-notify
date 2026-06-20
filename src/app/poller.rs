//! The polling engine: a configurable [`Poller`] that yields a [`Stream`] of [`Event`]s.

use std::time::Duration;

use chrono::Utc;
use futures::Stream;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::filter::Filters;
use super::store::{MemoryStore, StateStore};
use crate::client::Client;
use crate::error::{Error, Result};
use crate::models::{Notification, Reason, SubjectType};
use crate::pagination::Listing;

/// What the poller watches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollScope {
    /// The authenticated user's whole inbox.
    All,
    /// A single repository.
    Repo {
        /// Repository owner.
        owner: String,
        /// Repository name.
        name: String,
    },
}

impl PollScope {
    /// Watch a single repository.
    pub fn repo(owner: impl Into<String>, name: impl Into<String>) -> Self {
        PollScope::Repo {
            owner: owner.into(),
            name: name.into(),
        }
    }

    /// A stable key for this scope, used by [`StateStore`].
    pub fn key(&self) -> String {
        match self {
            PollScope::All => "all".to_owned(),
            PollScope::Repo { owner, name } => format!("repo:{owner}/{name}"),
        }
    }

    fn path(&self) -> String {
        match self {
            PollScope::All => "notifications".to_owned(),
            PollScope::Repo { owner, name } => format!("repos/{owner}/{name}/notifications"),
        }
    }
}

/// A notification surfaced by the poller.
#[derive(Debug, Clone)]
pub enum Event {
    /// A notification not seen before.
    New(Notification),
    /// A previously seen notification whose `updated_at` advanced.
    Updated(Notification),
}

impl Event {
    /// Borrow the underlying notification.
    pub fn notification(&self) -> &Notification {
        match self {
            Event::New(n) | Event::Updated(n) => n,
        }
    }

    /// Consume into the underlying notification.
    pub fn into_notification(self) -> Notification {
        match self {
            Event::New(n) | Event::Updated(n) => n,
        }
    }

    /// Whether this is a newly seen notification.
    pub fn is_new(&self) -> bool {
        matches!(self, Event::New(_))
    }
}

/// Configuration for a [`Poller`].
#[derive(Debug, Clone)]
pub struct PollConfig {
    /// What to watch.
    pub scope: PollScope,
    /// Include notifications already marked read (the `all` query parameter).
    pub include_read: bool,
    /// Only notifications the user participates in (the `participating` parameter).
    pub participating_only: bool,
    /// Never poll faster than this, even if the server suggests a shorter interval.
    pub min_interval: Duration,
    /// Honor the server's `X-Poll-Interval` (clamped up to `min_interval`).
    pub respect_server_interval: bool,
    /// Emit the existing inbox on the first tick, instead of only deltas afterward.
    pub emit_existing_on_start: bool,
}

impl Default for PollConfig {
    fn default() -> Self {
        PollConfig {
            scope: PollScope::All,
            include_read: false,
            participating_only: false,
            min_interval: Duration::from_secs(60),
            respect_server_interval: true,
            emit_existing_on_start: false,
        }
    }
}

/// Exponential backoff used between transient-failure retries.
#[derive(Debug, Clone)]
pub struct Backoff {
    /// Delay after the first failure.
    pub base: Duration,
    /// Maximum delay.
    pub max: Duration,
    /// Growth factor applied per consecutive failure.
    pub multiplier: f64,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            base: Duration::from_secs(1),
            max: Duration::from_secs(300),
            multiplier: 2.0,
        }
    }
}

impl Backoff {
    fn delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1) as i32;
        let secs = self.base.as_secs_f64() * self.multiplier.powi(exponent);
        Duration::from_secs_f64(secs.min(self.max.as_secs_f64()))
    }
}

/// How the poller reacts to errors during a tick.
///
/// The default retries transient failures forever with [`Backoff::default`] and does not
/// surface them as stream items.
#[derive(Debug, Clone, Default)]
pub struct ErrorPolicy {
    /// Stop after this many consecutive transient failures. `None` retries forever.
    pub max_consecutive_failures: Option<u32>,
    /// Backoff schedule for transient retries.
    pub backoff: Backoff,
    /// Surface transient errors as `Err` items (the stream still continues).
    pub emit_transient: bool,
}

/// A configured polling engine. Build one with [`Client::poller`].
pub struct Poller {
    client: Client,
    config: PollConfig,
    store: Box<dyn StateStore>,
    filters: Filters,
    error_policy: ErrorPolicy,
    cancel: Option<CancellationToken>,
}

impl Client {
    /// Start building a [`Poller`] over this client.
    pub fn poller(&self) -> PollerBuilder {
        PollerBuilder::new(self.clone())
    }
}

/// Builder for a [`Poller`].
pub struct PollerBuilder {
    client: Client,
    config: PollConfig,
    store: Option<Box<dyn StateStore>>,
    filters: Filters,
    error_policy: ErrorPolicy,
    cancel: Option<CancellationToken>,
}

impl PollerBuilder {
    fn new(client: Client) -> Self {
        PollerBuilder {
            client,
            config: PollConfig::default(),
            store: None,
            filters: Filters::default(),
            error_policy: ErrorPolicy::default(),
            cancel: None,
        }
    }

    /// Set what to watch (default: the whole inbox).
    pub fn scope(mut self, scope: PollScope) -> Self {
        self.config.scope = scope;
        self
    }

    /// Include already-read notifications.
    pub fn include_read(mut self, include_read: bool) -> Self {
        self.config.include_read = include_read;
        self
    }

    /// Only notifications the user participates in.
    pub fn participating_only(mut self, participating_only: bool) -> Self {
        self.config.participating_only = participating_only;
        self
    }

    /// The minimum interval between polls (default 60s).
    pub fn min_interval(mut self, interval: Duration) -> Self {
        self.config.min_interval = interval;
        self
    }

    /// Whether to honor the server's `X-Poll-Interval` (default true).
    pub fn respect_server_interval(mut self, respect: bool) -> Self {
        self.config.respect_server_interval = respect;
        self
    }

    /// Emit the existing inbox on the first tick instead of only deltas (default false).
    pub fn emit_existing_on_start(mut self, emit: bool) -> Self {
        self.config.emit_existing_on_start = emit;
        self
    }

    /// Use a custom [`StateStore`] (default: an in-memory store).
    pub fn store(mut self, store: impl StateStore + 'static) -> Self {
        self.store = Some(Box::new(store));
        self
    }

    /// Keep only notifications with one of these reasons.
    pub fn reasons(mut self, reasons: impl IntoIterator<Item = Reason>) -> Self {
        self.filters.add_reasons(reasons);
        self
    }

    /// Keep only notifications with one of these subject types.
    pub fn subject_types(mut self, types: impl IntoIterator<Item = SubjectType>) -> Self {
        self.filters.add_subject_types(types);
        self
    }

    /// Keep only notifications from this repository (useful with [`PollScope::All`]).
    pub fn repo_filter(mut self, owner: impl AsRef<str>, name: impl AsRef<str>) -> Self {
        self.filters.set_repo(owner.as_ref(), name.as_ref());
        self
    }

    /// Keep only notifications for which the predicate returns true.
    pub fn filter(
        mut self,
        predicate: impl Fn(&Notification) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.filters.add_predicate(std::sync::Arc::new(predicate));
        self
    }

    /// Configure error handling and backoff.
    pub fn error_policy(mut self, policy: ErrorPolicy) -> Self {
        self.error_policy = policy;
        self
    }

    /// Provide a cancellation token for cooperative shutdown.
    pub fn cancellation(mut self, token: CancellationToken) -> Self {
        self.cancel = Some(token);
        self
    }

    /// Build the [`Poller`].
    pub fn build(self) -> Poller {
        Poller {
            client: self.client,
            config: self.config,
            store: self.store.unwrap_or_else(|| Box::new(MemoryStore::new())),
            filters: self.filters,
            error_policy: self.error_policy,
            cancel: self.cancel,
        }
    }
}

impl Poller {
    /// Run the poller, yielding events until the stream is dropped, the cancellation token
    /// fires, or a fatal error ends it. See the robustness contract in the crate docs.
    pub fn stream(self) -> impl Stream<Item = Result<Event>> {
        let Poller {
            client,
            config,
            store,
            filters,
            error_policy,
            cancel,
        } = self;
        let scope = config.scope.clone();

        async_stream::stream! {
            let mut last_modified = match store.last_modified(&scope).await {
                Ok(value) => value,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };
            let mut first_tick = true;
            let mut failures: u32 = 0;

            loop {
                if let Some(token) = &cancel {
                    if token.is_cancelled() {
                        break;
                    }
                }

                let outcome = run_tick(&client, &config, last_modified.as_deref()).await;
                let (notifications, server_interval, tick_last_modified) = match outcome {
                    Ok(TickOutcome::NotModified { server_interval }) => {
                        // Our watermark is current; nothing to seed or emit.
                        first_tick = false;
                        let interval = effective_interval(&config, server_interval);
                        if sleep_or_cancel(interval, &cancel).await {
                            break;
                        }
                        continue;
                    }
                    Ok(TickOutcome::Modified {
                        notifications,
                        server_interval,
                        last_modified,
                    }) => (notifications, server_interval, last_modified),
                    Err(e) => {
                        if !is_transient(&e) {
                            yield Err(e);
                            break;
                        }
                        failures += 1;
                        let exceeded = error_policy
                            .max_consecutive_failures
                            .is_some_and(|max| failures > max);
                        let delay = retry_delay(&e, &error_policy.backoff, failures);
                        if error_policy.emit_transient || exceeded {
                            yield Err(e);
                        }
                        if exceeded {
                            break;
                        }
                        if sleep_or_cancel(delay, &cancel).await {
                            break;
                        }
                        continue;
                    }
                };
                failures = 0;

                let mut sorted = notifications;
                sorted.sort_by_key(|n| n.updated_at);

                for n in sorted {
                    if !filters.matches(&n) {
                        continue;
                    }
                    let seen = match store.seen(&n.id).await {
                        Ok(seen) => seen,
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    };
                    let event = match seen {
                        None => Some(Event::New(n.clone())),
                        Some(prev) if n.updated_at > prev => Some(Event::Updated(n.clone())),
                        Some(_) => None,
                    };
                    let Some(event) = event else {
                        continue;
                    };

                    if first_tick && !config.emit_existing_on_start {
                        // Seed the store without emitting the pre-existing inbox.
                        if let Err(e) = store.record_seen(&n.id, n.updated_at).await {
                            yield Err(e);
                            return;
                        }
                        continue;
                    }

                    // Yield first, then record: a crash before recording re-emits on the
                    // next run (at-least-once) rather than dropping the event.
                    yield Ok(event);
                    if let Err(e) = store.record_seen(&n.id, n.updated_at).await {
                        yield Err(e);
                        return;
                    }
                }

                // Advance the watermark only after the whole tick is processed.
                if let Some(value) = tick_last_modified {
                    last_modified = Some(value.clone());
                    if let Err(e) = store.set_last_modified(&scope, &value).await {
                        yield Err(e);
                        return;
                    }
                }
                first_tick = false;

                let interval = effective_interval(&config, server_interval);
                if sleep_or_cancel(interval, &cancel).await {
                    break;
                }
            }
        }
    }
}

enum TickOutcome {
    NotModified {
        server_interval: Option<Duration>,
    },
    Modified {
        notifications: Vec<Notification>,
        server_interval: Option<Duration>,
        last_modified: Option<String>,
    },
}

/// Fetch one tick: a conditional first page, then follow pagination unconditionally so a
/// multi-page inbox is never half-read.
async fn run_tick(
    client: &Client,
    config: &PollConfig,
    if_modified_since: Option<&str>,
) -> Result<TickOutcome> {
    let url = build_list_url(client, config)?;
    let first = client
        .execute_list::<Notification>(url, if_modified_since)
        .await?;

    let mut page = match first {
        Listing::NotModified(nm) => {
            return Ok(TickOutcome::NotModified {
                server_interval: nm.poll_interval,
            });
        }
        Listing::Modified(page) => page,
    };

    let server_interval = page.poll_interval;
    let last_modified = page.last_modified.clone();
    let mut notifications = std::mem::take(&mut page.items);
    let mut next = page.next;
    while let Some(url) = next {
        match client.execute_list::<Notification>(url, None).await? {
            Listing::Modified(p) => {
                notifications.extend(p.items);
                next = p.next;
            }
            Listing::NotModified(_) => break,
        }
    }

    Ok(TickOutcome::Modified {
        notifications,
        server_interval,
        last_modified,
    })
}

fn build_list_url(client: &Client, config: &PollConfig) -> Result<Url> {
    let mut url = client.endpoint(&config.scope.path())?;
    {
        let mut query = url.query_pairs_mut();
        if config.include_read {
            query.append_pair("all", "true");
        }
        if config.participating_only {
            query.append_pair("participating", "true");
        }
    }
    Ok(url)
}

fn effective_interval(config: &PollConfig, server: Option<Duration>) -> Duration {
    let base = if config.respect_server_interval {
        server.unwrap_or(config.min_interval)
    } else {
        config.min_interval
    };
    base.max(config.min_interval)
}

fn is_transient(e: &Error) -> bool {
    match e {
        Error::Http(_) => true,
        Error::RateLimited { .. } => true,
        Error::Api { status, .. } => status.is_server_error(),
        _ => false,
    }
}

/// Choose how long to wait after a transient failure: honor rate-limit hints when present,
/// otherwise fall back to exponential backoff.
fn retry_delay(e: &Error, backoff: &Backoff, attempt: u32) -> Duration {
    if let Error::RateLimited {
        retry_after,
        reset_at,
        ..
    } = e
    {
        if let Some(after) = retry_after {
            return *after;
        }
        if let Some(reset) = reset_at {
            if let Ok(until) = (*reset - Utc::now()).to_std() {
                return until.min(Duration::from_secs(3600));
            }
        }
    }
    backoff.delay(attempt)
}

/// Sleep for `interval`, returning `true` if the cancellation token fired first.
async fn sleep_or_cancel(interval: Duration, cancel: &Option<CancellationToken>) -> bool {
    match cancel {
        Some(token) => tokio::time::timeout(interval, token.cancelled())
            .await
            .is_ok(),
        None => {
            tokio::time::sleep(interval).await;
            false
        }
    }
}

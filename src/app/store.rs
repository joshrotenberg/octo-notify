//! The [`StateStore`] trait and an in-memory implementation.
//!
//! The store lets a long-running poller dedupe across restarts: it remembers the
//! `Last-Modified` watermark per scope (so conditional polling resumes) and the last
//! `updated_at` seen per thread (so already-delivered notifications aren't re-emitted).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::PollScope;
use crate::error::Result;
use crate::models::ThreadId;

/// Persistence for poller state.
///
/// Implementations must be cheap to share (`Send + Sync`); the poller calls them on every
/// tick. The provided [`MemoryStore`] is process-local; persist to disk or a database for
/// dedupe that survives restarts.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// The stored `Last-Modified` watermark for a scope, if any.
    async fn last_modified(&self, scope: &PollScope) -> Result<Option<String>>;

    /// Store the `Last-Modified` watermark for a scope.
    async fn set_last_modified(&self, scope: &PollScope, value: &str) -> Result<()>;

    /// The last `updated_at` seen for a thread, if it has been seen.
    async fn seen(&self, id: &ThreadId) -> Result<Option<DateTime<Utc>>>;

    /// Record that a thread was seen at `updated_at`.
    async fn record_seen(&self, id: &ThreadId, updated_at: DateTime<Utc>) -> Result<()>;

    /// Drop seen-records older than `older_than` to bound memory/disk.
    async fn prune(&self, older_than: DateTime<Utc>) -> Result<()>;
}

/// A process-local, in-memory [`StateStore`]. Resets when the process exits.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    inner: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    last_modified: HashMap<String, String>,
    seen: HashMap<ThreadId, DateTime<Utc>>,
}

impl MemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl StateStore for MemoryStore {
    async fn last_modified(&self, scope: &PollScope) -> Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .last_modified
            .get(&scope.key())
            .cloned())
    }

    async fn set_last_modified(&self, scope: &PollScope, value: &str) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .last_modified
            .insert(scope.key(), value.to_owned());
        Ok(())
    }

    async fn seen(&self, id: &ThreadId) -> Result<Option<DateTime<Utc>>> {
        Ok(self.inner.lock().unwrap().seen.get(id).copied())
    }

    async fn record_seen(&self, id: &ThreadId, updated_at: DateTime<Utc>) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .seen
            .insert(id.clone(), updated_at);
        Ok(())
    }

    async fn prune(&self, older_than: DateTime<Utc>) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .seen
            .retain(|_, ts| *ts >= older_than);
        Ok(())
    }
}

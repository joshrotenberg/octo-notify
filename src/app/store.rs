//! The [`StateStore`] trait and an in-memory implementation.
//!
//! The store lets a long-running poller dedupe across restarts: it remembers the
//! `Last-Modified` watermark per scope (so conditional polling resumes) and the last
//! `updated_at` seen per thread (so already-delivered notifications aren't re-emitted).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::PollScope;
use crate::error::Result;
use crate::models::ThreadId;

#[cfg(feature = "file-store")]
use crate::error::Error;
#[cfg(feature = "file-store")]
use std::path::PathBuf;

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    last_modified: HashMap<String, String>,
    #[serde(default)]
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

/// A [`StateStore`] backed by a JSON file, so poller state survives process restarts.
///
/// State is loaded on [`open`](JsonFileStore::open) and written back after every mutating
/// call. Writes are atomic: a temp file in the same directory is written and then renamed over
/// the target, so a crash mid-write cannot corrupt the existing file. Requires the
/// `file-store` feature.
#[cfg(feature = "file-store")]
#[derive(Debug, Clone)]
pub struct JsonFileStore {
    path: PathBuf,
    inner: Arc<Mutex<State>>,
}

#[cfg(feature = "file-store")]
impl JsonFileStore {
    /// Open the store at `path`, loading any existing state. A missing file starts empty; a
    /// corrupt or unreadable file returns an error.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let state = match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice::<State>(&bytes).map_err(|source| Error::Deserialize {
                    source,
                    body: String::from_utf8_lossy(&bytes).into_owned(),
                })?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(Error::Io(e)),
        };
        Ok(JsonFileStore {
            path,
            inner: Arc::new(Mutex::new(state)),
        })
    }

    fn temp_path(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    }

    /// Serialize `state` and write it atomically (temp file in the same directory, then
    /// rename over the target).
    fn persist(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let temp = self.temp_path();
        std::fs::write(&temp, &bytes)?;
        std::fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

#[cfg(feature = "file-store")]
#[async_trait]
impl StateStore for JsonFileStore {
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
        let mut guard = self.inner.lock().unwrap();
        guard.last_modified.insert(scope.key(), value.to_owned());
        self.persist(&guard)
    }

    async fn seen(&self, id: &ThreadId) -> Result<Option<DateTime<Utc>>> {
        Ok(self.inner.lock().unwrap().seen.get(id).copied())
    }

    async fn record_seen(&self, id: &ThreadId, updated_at: DateTime<Utc>) -> Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.seen.insert(id.clone(), updated_at);
        self.persist(&guard)
    }

    async fn prune(&self, older_than: DateTime<Utc>) -> Result<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.seen.retain(|_, ts| *ts >= older_than);
        self.persist(&guard)
    }
}

//! Bulk operations over many notification threads.

use futures::stream::{self, StreamExt};

use crate::client::Client;
use crate::error::Result;
use crate::models::ThreadId;

#[derive(Clone, Copy)]
enum MarkKind {
    Read,
    Done,
}

impl Client {
    /// Mark many threads as read concurrently, with bounded concurrency.
    ///
    /// Returns one `(ThreadId, Result<()>)` per input id; results are in completion order and
    /// each carries its id, so a caller can map by id. A single failure does not abort the
    /// batch. `concurrency` is clamped to at least 1. Requires the `stream` feature.
    pub async fn mark_read_each(
        &self,
        ids: impl IntoIterator<Item = ThreadId>,
        concurrency: usize,
    ) -> Vec<(ThreadId, Result<()>)> {
        self.mark_each(ids, concurrency, MarkKind::Read).await
    }

    /// Mark many threads as done concurrently, with bounded concurrency.
    ///
    /// Like [`mark_read_each`](Client::mark_read_each) but issues the one-way "mark as done"
    /// (`DELETE`) for each thread.
    pub async fn mark_done_each(
        &self,
        ids: impl IntoIterator<Item = ThreadId>,
        concurrency: usize,
    ) -> Vec<(ThreadId, Result<()>)> {
        self.mark_each(ids, concurrency, MarkKind::Done).await
    }

    async fn mark_each(
        &self,
        ids: impl IntoIterator<Item = ThreadId>,
        concurrency: usize,
        kind: MarkKind,
    ) -> Vec<(ThreadId, Result<()>)> {
        let concurrency = concurrency.max(1);
        stream::iter(ids)
            .map(move |id| async move {
                let result = match kind {
                    MarkKind::Read => self.thread(id.clone()).mark_read().await,
                    MarkKind::Done => self.thread(id.clone()).mark_done().await,
                };
                (id, result)
            })
            .buffer_unordered(concurrency)
            .collect()
            .await
    }
}

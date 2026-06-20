//! Layer 2/3: the application engine.
//!
//! A [`Poller`] turns the inbox into an async [`Stream`](futures::Stream) of [`Event`]s,
//! deduping through a [`StateStore`] so a long-running app doesn't re-notify across
//! restarts. See the robustness contract in the crate-root docs.

mod filter;
mod poller;
mod store;

pub use poller::{Backoff, ErrorPolicy, Event, PollConfig, PollScope, Poller, PollerBuilder};
pub use store::{MemoryStore, StateStore};

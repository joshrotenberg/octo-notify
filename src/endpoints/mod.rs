//! Typed endpoint handlers.

#[cfg(feature = "stream")]
mod bulk;
pub mod notifications;
pub mod repo;
pub mod subscriptions;
pub mod threads;

pub use notifications::{ListNotifications, MarkAllRead, NotificationsHandler};
pub use repo::{RepoHandler, RepoNotificationsHandler};
pub use subscriptions::ListSubscriptions;
pub use threads::ThreadHandler;

//! Typed endpoint handlers.

pub mod notifications;
pub mod repo;
pub mod threads;

pub use notifications::{ListNotifications, MarkAllRead, NotificationsHandler};
pub use repo::{RepoHandler, RepoNotificationsHandler};
pub use threads::ThreadHandler;

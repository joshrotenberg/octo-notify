//! Typed models for notification payloads.
//!
//! The `Reason` and `SubjectType` enums are *forward-compatible*: a value GitHub adds in
//! the future deserializes into the `Unknown(String)` variant instead of failing, so a new
//! notification reason can never break deserialization.

mod notification;
mod reason;
mod repository;
mod subject;
mod subscription;

pub use notification::{Notification, ThreadId};
pub use reason::Reason;
pub use repository::{MinimalRepository, SimpleUser};
pub use subject::{Subject, SubjectType};
pub use subscription::ThreadSubscription;

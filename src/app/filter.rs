//! Client-side filtering applied to notifications during polling.
//!
//! The API only filters by `all`/`participating`/`since`, so reason/type/repo filtering
//! happens here. Filter categories are ANDed; within a category (e.g. reasons) membership
//! is ORed, so `reasons([Mention, ReviewRequested])` keeps either.

use std::collections::HashSet;
use std::sync::Arc;

use crate::models::{Notification, Reason, SubjectType};

type Predicate = Arc<dyn Fn(&Notification) -> bool + Send + Sync>;

/// A set of filters combined with AND across categories.
#[derive(Clone, Default)]
pub(crate) struct Filters {
    reasons: Option<HashSet<Reason>>,
    subject_types: Option<HashSet<SubjectType>>,
    repo: Option<String>,
    predicates: Vec<Predicate>,
}

impl Filters {
    pub(crate) fn add_reasons(&mut self, reasons: impl IntoIterator<Item = Reason>) {
        self.reasons
            .get_or_insert_with(HashSet::new)
            .extend(reasons);
    }

    pub(crate) fn add_subject_types(&mut self, types: impl IntoIterator<Item = SubjectType>) {
        self.subject_types
            .get_or_insert_with(HashSet::new)
            .extend(types);
    }

    pub(crate) fn set_repo(&mut self, owner: &str, name: &str) {
        self.repo = Some(format!("{owner}/{name}"));
    }

    pub(crate) fn add_predicate(&mut self, predicate: Predicate) {
        self.predicates.push(predicate);
    }

    /// Whether a notification passes every configured filter.
    pub(crate) fn matches(&self, n: &Notification) -> bool {
        if let Some(reasons) = &self.reasons {
            if !reasons.contains(&n.reason) {
                return false;
            }
        }
        if let Some(types) = &self.subject_types {
            if !types.contains(&n.subject.kind) {
                return false;
            }
        }
        if let Some(repo) = &self.repo {
            if &n.repository.full_name != repo {
                return false;
            }
        }
        self.predicates.iter().all(|p| p(n))
    }
}

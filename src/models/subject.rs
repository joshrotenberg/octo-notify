//! The notification subject and its type.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// What a notification is about (the issue, PR, commit, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    /// The subject's title.
    pub title: String,
    /// API URL of the subject. Absent for some subject types.
    #[serde(default)]
    pub url: Option<Url>,
    /// API URL of the latest comment on the subject, if any.
    #[serde(default)]
    pub latest_comment_url: Option<Url>,
    /// The kind of subject.
    #[serde(rename = "type")]
    pub kind: SubjectType,
}

impl Subject {
    /// `true` if the subject is a pull request.
    pub fn is_pull_request(&self) -> bool {
        matches!(self.kind, SubjectType::PullRequest)
    }

    /// Best-effort issue/PR number parsed from the trailing path segment of [`Subject::url`].
    pub fn issue_number(&self) -> Option<u64> {
        self.url
            .as_ref()?
            .path_segments()?
            .next_back()?
            .parse()
            .ok()
    }
}

/// The type of a notification subject.
///
/// Forward-compatible: unrecognized values land in [`SubjectType::Unknown`].
///
/// ```
/// use octo_notify::SubjectType;
/// let kind: SubjectType = serde_json::from_str("\"PullRequest\"").unwrap();
/// assert_eq!(kind, SubjectType::PullRequest);
/// let future: SubjectType = serde_json::from_str("\"FutureType\"").unwrap();
/// assert!(future.is_unknown());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SubjectType {
    /// An issue.
    Issue,
    /// A pull request.
    PullRequest,
    /// A commit.
    Commit,
    /// A release.
    Release,
    /// A discussion.
    Discussion,
    /// A Dependabot / vulnerability alert.
    RepositoryVulnerabilityAlert,
    /// A check suite (CI).
    CheckSuite,
    /// A repository invitation.
    RepositoryInvitation,
    /// A type not known to this crate version. Holds the raw string.
    Unknown(String),
}

impl SubjectType {
    /// The wire representation of this subject type.
    pub fn as_str(&self) -> &str {
        match self {
            SubjectType::Issue => "Issue",
            SubjectType::PullRequest => "PullRequest",
            SubjectType::Commit => "Commit",
            SubjectType::Release => "Release",
            SubjectType::Discussion => "Discussion",
            SubjectType::RepositoryVulnerabilityAlert => "RepositoryVulnerabilityAlert",
            SubjectType::CheckSuite => "CheckSuite",
            SubjectType::RepositoryInvitation => "RepositoryInvitation",
            SubjectType::Unknown(s) => s,
        }
    }

    /// Whether this type was not recognized by this crate version.
    pub fn is_unknown(&self) -> bool {
        matches!(self, SubjectType::Unknown(_))
    }

    fn from_wire(s: &str) -> Self {
        match s {
            "Issue" => SubjectType::Issue,
            "PullRequest" => SubjectType::PullRequest,
            "Commit" => SubjectType::Commit,
            "Release" => SubjectType::Release,
            "Discussion" => SubjectType::Discussion,
            "RepositoryVulnerabilityAlert" => SubjectType::RepositoryVulnerabilityAlert,
            "CheckSuite" => SubjectType::CheckSuite,
            "RepositoryInvitation" => SubjectType::RepositoryInvitation,
            other => SubjectType::Unknown(other.to_owned()),
        }
    }
}

impl fmt::Display for SubjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SubjectType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(SubjectType::from_wire(&raw))
    }
}

impl Serialize for SubjectType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_types() {
        let known: SubjectType = serde_json::from_str("\"PullRequest\"").unwrap();
        assert_eq!(known, SubjectType::PullRequest);

        let unknown: SubjectType = serde_json::from_str("\"MysteryType\"").unwrap();
        assert_eq!(unknown, SubjectType::Unknown("MysteryType".to_owned()));
    }

    #[test]
    fn issue_number_parsed_from_url() {
        let subject: Subject = serde_json::from_str(
            r#"{"title":"x","url":"https://api.github.com/repos/o/r/issues/42","latest_comment_url":null,"type":"Issue"}"#,
        )
        .unwrap();
        assert_eq!(subject.issue_number(), Some(42));
    }
}

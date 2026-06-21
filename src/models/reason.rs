//! The notification `reason` enum.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why a notification was delivered.
///
/// Forward-compatible: unrecognized values land in [`Reason::Unknown`] rather than failing
/// to deserialize. The `reason` on a thread can change over time if a later event has a
/// different reason.
///
/// ```
/// use octo_notify::Reason;
/// // A value this crate version does not know is captured, not rejected.
/// let reason: Reason = serde_json::from_str("\"a_future_reason\"").unwrap();
/// assert!(reason.is_unknown());
/// assert_eq!(reason.as_str(), "a_future_reason");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Reason {
    /// You were assigned to the issue.
    Assign,
    /// You created the thread.
    Author,
    /// You commented on the thread.
    Comment,
    /// A workflow run you triggered finished.
    CiActivity,
    /// You accepted an invitation to contribute.
    Invitation,
    /// You subscribed to the thread manually.
    Manual,
    /// You were @mentioned.
    Mention,
    /// You (or a team you're on) were requested to review a PR.
    ReviewRequested,
    /// A security vulnerability was found in one of your repositories.
    SecurityAlert,
    /// The thread's state changed (issue closed, PR merged, ...).
    StateChange,
    /// You're watching the repository.
    Subscribed,
    /// A team you're on was @mentioned.
    TeamMention,
    /// You were requested to review and approve a deployment.
    ApprovalRequested,
    /// Organization members requested a feature be enabled.
    MemberFeatureRequested,
    /// You were credited for a security advisory.
    SecurityAdvisoryCredit,
    /// Activity on something resulting from your own action.
    YourActivity,
    /// A reason not known to this crate version. Holds the raw string.
    Unknown(String),
}

impl Reason {
    /// The wire representation of this reason.
    pub fn as_str(&self) -> &str {
        match self {
            Reason::Assign => "assign",
            Reason::Author => "author",
            Reason::Comment => "comment",
            Reason::CiActivity => "ci_activity",
            Reason::Invitation => "invitation",
            Reason::Manual => "manual",
            Reason::Mention => "mention",
            Reason::ReviewRequested => "review_requested",
            Reason::SecurityAlert => "security_alert",
            Reason::StateChange => "state_change",
            Reason::Subscribed => "subscribed",
            Reason::TeamMention => "team_mention",
            Reason::ApprovalRequested => "approval_requested",
            Reason::MemberFeatureRequested => "member_feature_requested",
            Reason::SecurityAdvisoryCredit => "security_advisory_credit",
            Reason::YourActivity => "your_activity",
            Reason::Unknown(s) => s,
        }
    }

    /// Whether this reason was not recognized by this crate version.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Reason::Unknown(_))
    }

    fn from_wire(s: &str) -> Self {
        match s {
            "assign" => Reason::Assign,
            "author" => Reason::Author,
            "comment" => Reason::Comment,
            "ci_activity" => Reason::CiActivity,
            "invitation" => Reason::Invitation,
            "manual" => Reason::Manual,
            "mention" => Reason::Mention,
            "review_requested" => Reason::ReviewRequested,
            "security_alert" => Reason::SecurityAlert,
            "state_change" => Reason::StateChange,
            "subscribed" => Reason::Subscribed,
            "team_mention" => Reason::TeamMention,
            "approval_requested" => Reason::ApprovalRequested,
            "member_feature_requested" => Reason::MemberFeatureRequested,
            "security_advisory_credit" => Reason::SecurityAdvisoryCredit,
            "your_activity" => Reason::YourActivity,
            other => Reason::Unknown(other.to_owned()),
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Reason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Reason::from_wire(&raw))
    }
}

impl Serialize for Reason {
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
    fn known_reason_roundtrips() {
        let r: Reason = serde_json::from_str("\"review_requested\"").unwrap();
        assert_eq!(r, Reason::ReviewRequested);
        assert_eq!(serde_json::to_string(&r).unwrap(), "\"review_requested\"");
    }

    #[test]
    fn unknown_reason_is_captured_not_rejected() {
        let r: Reason = serde_json::from_str("\"brand_new_reason_2099\"").unwrap();
        assert_eq!(r, Reason::Unknown("brand_new_reason_2099".to_owned()));
        assert!(r.is_unknown());
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            "\"brand_new_reason_2099\""
        );
    }
}

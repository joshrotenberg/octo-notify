//! Small shared CLI helpers.

/// Split an `owner/name` repository argument, rejecting anything malformed.
pub(crate) fn split_repo(repo: &str) -> anyhow::Result<(&str, &str)> {
    match repo.split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok((owner, name))
        }
        _ => anyhow::bail!("expected repository as \"owner/name\", got {repo:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_repo_accepts_owner_name_and_rejects_the_rest() {
        assert_eq!(split_repo("octocat/hello").unwrap(), ("octocat", "hello"));
        for bad in ["octocat", "octocat/", "/hello", "a/b/c", ""] {
            assert!(split_repo(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}

//! Repository subscription commands: `subscribe`, `unsubscribe`, and `subscription`.

use super::util::split_repo;
use crate::Client;

pub(crate) async fn subscribe(client: &Client, repo: &str, ignore: bool) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    let handler = client.repo(owner, name);
    let sub = if ignore {
        handler.ignore().await?
    } else {
        handler.subscribe().await?
    };
    println!("{repo}: {}", state(sub.subscribed, sub.ignored));
    Ok(())
}

pub(crate) async fn unsubscribe(client: &Client, repo: &str) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    client.repo(owner, name).delete_subscription().await?;
    println!("{repo}: unsubscribed");
    Ok(())
}

pub(crate) async fn status(client: &Client, repo: &str) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    match client.repo(owner, name).subscription().await {
        Ok(sub) => {
            println!("{repo}: {}", state(sub.subscribed, sub.ignored));
            Ok(())
        }
        // 404 is the API's way of saying "no subscription exists" for this repo.
        Err(crate::Error::Api { status, .. }) if status.as_u16() == 404 => {
            println!("{repo}: not subscribed");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn state(subscribed: bool, ignored: bool) -> &'static str {
    if ignored {
        "ignored"
    } else if subscribed {
        "watching"
    } else {
        "not subscribed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_labels() {
        assert_eq!(state(true, false), "watching");
        assert_eq!(state(false, true), "ignored");
        assert_eq!(state(true, true), "ignored");
        assert_eq!(state(false, false), "not subscribed");
    }
}

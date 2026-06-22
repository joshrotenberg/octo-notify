//! The `mark-read` command: mark the whole inbox, or one repository, as read.

use super::util::split_repo;
use crate::Client;

pub(crate) async fn run(client: &Client, repo: Option<&str>) -> anyhow::Result<()> {
    match repo {
        Some(r) => {
            let (owner, name) = split_repo(r)?;
            client
                .repo(owner, name)
                .notifications()
                .mark_all_read()
                .send()
                .await?;
            println!("{r}: marked read");
        }
        None => {
            client.notifications().mark_all_read().send().await?;
            println!("inbox: marked read");
        }
    }
    Ok(())
}

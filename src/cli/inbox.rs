//! The `inbox` command.

use chrono::{DateTime, Utc};

use super::util::split_repo;
use crate::{Client, ListNotifications, Listing};

pub(crate) struct InboxArgs {
    pub(crate) all: bool,
    pub(crate) participating: bool,
    pub(crate) per_page: u8,
    pub(crate) repo: Option<String>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) before: Option<DateTime<Utc>>,
    pub(crate) page: Option<u32>,
}

/// Apply the shared inbox filters to a listing builder, whichever scope it came from.
fn apply_filters<'a>(list: ListNotifications<'a>, args: &InboxArgs) -> ListNotifications<'a> {
    let mut list = list
        .include_read(args.all)
        .participating(args.participating)
        .per_page(args.per_page);
    if let Some(since) = args.since {
        list = list.since(since);
    }
    if let Some(before) = args.before {
        list = list.before(before);
    }
    if let Some(page) = args.page {
        list = list.page(page);
    }
    list
}

pub(crate) async fn run(client: &Client, args: InboxArgs) -> anyhow::Result<()> {
    let listing = match args.repo.as_deref() {
        Some(repo) => {
            let (owner, name) = split_repo(repo)?;
            apply_filters(client.repo(owner, name).notifications().list(), &args)
                .send()
                .await?
        }
        None => {
            apply_filters(client.notifications().list(), &args)
                .send()
                .await?
        }
    };

    match listing {
        Listing::Modified(page) => {
            let rl = &page.rate_limit;
            println!(
                "{} notification(s) · rate limit {}/{} remaining\n",
                page.items.len(),
                rl.remaining
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into()),
                rl.limit
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into()),
            );
            for n in &page.items {
                let flag = if n.unread { "●" } else { "○" };
                println!(
                    "{flag} [{:>16}] {:<28} {}  ({})",
                    n.reason.to_string(),
                    n.repository.full_name,
                    n.subject.title,
                    n.subject.kind,
                );
            }
            if page.has_next() {
                println!("\n(more pages available; raise --per-page or paginate)");
            }
        }
        Listing::NotModified(_) => println!("not modified"),
    }
    Ok(())
}

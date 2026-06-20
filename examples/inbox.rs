//! A small worked CLI over octo-notify: list your GitHub notifications.
//!
//! Run it against your real account:
//!
//! ```text
//! GITHUB_TOKEN=$(gh auth token) cargo run --example inbox -- --participating
//! ```
//!
//! This is intentionally a single-file example. It is the seed of a standalone CLI tool
//! that could be spun out into its own crate later.

use clap::Parser;
use octo_notify::{Auth, Client, Listing};

/// List your GitHub notifications.
#[derive(Parser, Debug)]
#[command(name = "octo-inbox", about = "List your GitHub notifications")]
struct Args {
    /// Include notifications already marked as read.
    #[arg(long)]
    all: bool,

    /// Only notifications you're directly participating in.
    #[arg(long)]
    participating: bool,

    /// Results per page (max 50).
    #[arg(long, default_value_t = 50)]
    per_page: u8,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = Client::new(Auth::from_env()?)?;
    let listing = client
        .notifications()
        .list()
        .all(args.all)
        .participating(args.participating)
        .per_page(args.per_page)
        .send()
        .await?;

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
                println!("\n(more pages available — pagination lands in M2)");
            }
        }
        Listing::NotModified(_) => println!("not modified"),
    }

    Ok(())
}

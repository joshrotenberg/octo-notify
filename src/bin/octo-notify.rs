//! The `octo-notify` command-line tool, built behind the `cli` feature:
//!
//! ```text
//! cargo install octo-notify --features cli
//! GITHUB_TOKEN=$(gh auth token) octo-notify inbox --all
//! GITHUB_TOKEN=$(gh auth token) octo-notify watch --state ~/.cache/octo-notify.json
//! ```

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use futures::StreamExt;
use octo_notify::app::{Event, JsonFileStore};
use octo_notify::{Auth, Client, Listing};
use tokio_util::sync::CancellationToken;

/// Work with your GitHub notifications inbox.
#[derive(Parser, Debug)]
#[command(name = "octo-notify", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List notifications.
    Inbox {
        /// Include notifications already marked as read.
        #[arg(long)]
        all: bool,
        /// Only notifications you're directly participating in.
        #[arg(long)]
        participating: bool,
        /// Results per page (max 50).
        #[arg(long, default_value_t = 50)]
        per_page: u8,
    },
    /// Watch notifications as a live stream.
    Watch {
        /// Minimum seconds between polls (the server may request longer).
        #[arg(long, default_value_t = 60)]
        interval: u64,
        /// Only notifications you're directly participating in.
        #[arg(long)]
        participating: bool,
        /// Include notifications already marked as read.
        #[arg(long)]
        all: bool,
        /// Emit the current inbox once on start, instead of only new activity.
        #[arg(long)]
        show_existing: bool,
        /// Persist dedupe state to this file so restarts resume without re-firing.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = Client::new(Auth::from_env()?)?;
    match cli.command {
        Command::Inbox {
            all,
            participating,
            per_page,
        } => inbox(&client, all, participating, per_page).await,
        Command::Watch {
            interval,
            participating,
            all,
            show_existing,
            state,
        } => watch(client, interval, participating, all, show_existing, state).await,
    }
}

async fn inbox(
    client: &Client,
    all: bool,
    participating: bool,
    per_page: u8,
) -> anyhow::Result<()> {
    let listing = client
        .notifications()
        .list()
        .include_read(all)
        .participating(participating)
        .per_page(per_page)
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
                println!("\n(more pages available; raise --per-page or paginate)");
            }
        }
        Listing::NotModified(_) => println!("not modified"),
    }
    Ok(())
}

async fn watch(
    client: Client,
    interval: u64,
    participating: bool,
    all: bool,
    show_existing: bool,
    state: Option<PathBuf>,
) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down at next tick...");
        shutdown.cancel();
    });

    let builder = client
        .poller()
        .min_interval(Duration::from_secs(interval))
        .participating_only(participating)
        .include_read(all)
        .emit_existing_on_start(show_existing)
        .cancellation(cancel);
    let builder = match state {
        Some(path) => {
            eprintln!("persisting state to {}", path.display());
            builder.store(JsonFileStore::open(path)?)
        }
        None => builder,
    };
    let poller = builder.build();

    eprintln!("watching (interval >= {interval}s); Ctrl-C to stop");
    let mut events = Box::pin(poller.stream());
    while let Some(event) = events.next().await {
        match event {
            Ok(Event::New(n)) => println!(
                "NEW      [{}] {} - {}",
                n.reason, n.repository.full_name, n.subject.title
            ),
            Ok(Event::Updated(n)) => println!(
                "UPDATED  [{}] {} - {}",
                n.reason, n.repository.full_name, n.subject.title
            ),
            Err(e) => eprintln!("error: {e}"),
        }
    }
    Ok(())
}

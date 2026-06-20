//! Watch your GitHub notifications as a live stream.
//!
//! ```text
//! GITHUB_TOKEN=$(gh auth token) cargo run --example watch -- --interval 60
//! ```
//!
//! Press Ctrl-C to stop; the poller shuts down cooperatively at the next tick boundary.
//! Like `inbox`, this is a seed for a standalone CLI that could be spun out later.

use std::time::Duration;

use clap::Parser;
use futures::StreamExt;
use octo_notify::app::Event;
use octo_notify::{Auth, Client};
use tokio_util::sync::CancellationToken;

/// Watch GitHub notifications.
#[derive(Parser, Debug)]
#[command(name = "octo-watch", about = "Stream new GitHub notifications")]
struct Args {
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = Client::new(Auth::from_env()?)?;

    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down at next tick...");
        shutdown.cancel();
    });

    let poller = client
        .poller()
        .min_interval(Duration::from_secs(args.interval))
        .participating_only(args.participating)
        .include_read(args.all)
        .emit_existing_on_start(args.show_existing)
        .cancellation(cancel)
        .build();

    eprintln!("watching (interval >= {}s); Ctrl-C to stop", args.interval);
    let mut events = Box::pin(poller.stream());
    while let Some(event) = events.next().await {
        match event {
            Ok(Event::New(n)) => {
                println!(
                    "NEW      [{}] {} - {}",
                    n.reason, n.repository.full_name, n.subject.title
                );
            }
            Ok(Event::Updated(n)) => {
                println!(
                    "UPDATED  [{}] {} - {}",
                    n.reason, n.repository.full_name, n.subject.title
                );
            }
            Err(e) => eprintln!("error: {e}"),
        }
    }

    Ok(())
}

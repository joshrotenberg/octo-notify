//! The `octo-notify` command-line tool, built behind the `cli` feature.
//!
//! The binary at `src/bin/octo-notify.rs` is a thin shim over [`run`]; all command logic lives
//! here, one module per command area, so it can be exercised without spawning a process.

mod args;
mod inbox;
mod mark_read;
mod subscribe;
mod subscriptions;
mod thread;
mod util;
mod watch;

use clap::Parser;

use crate::{Auth, Client};
use args::{Cli, Command};

/// Parse arguments, build a [`Client`](crate::Client) from the environment, and run the
/// requested command. This is the entry point the `octo-notify` binary calls.
#[tokio::main]
pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = Client::new(Auth::from_env()?)?;
    match cli.command {
        Command::Inbox {
            all,
            participating,
            per_page,
            repo,
            since,
            before,
            page,
        } => {
            inbox::run(
                &client,
                inbox::InboxArgs {
                    all,
                    participating,
                    per_page,
                    repo,
                    since,
                    before,
                    page,
                },
            )
            .await
        }
        Command::Watch {
            interval,
            participating,
            all,
            show_existing,
            state,
            rules,
        } => {
            watch::run(
                client,
                watch::WatchArgs {
                    interval,
                    participating,
                    all,
                    show_existing,
                    state,
                    rules,
                },
            )
            .await
        }
        Command::Subscribe { repo, ignore } => subscribe::subscribe(&client, &repo, ignore).await,
        Command::Unsubscribe { repo } => subscribe::unsubscribe(&client, &repo).await,
        Command::Subscription { repo } => subscribe::status(&client, &repo).await,
        Command::Subscriptions => subscriptions::run(&client).await,
        Command::MarkRead { repo } => mark_read::run(&client, repo.as_deref()).await,
        Command::Thread { action } => thread::run(&client, action).await,
    }
}

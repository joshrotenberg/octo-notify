//! clap argument definitions for the CLI.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};

/// Work with your GitHub notifications inbox.
#[derive(Parser, Debug)]
#[command(name = "octo-notify", version, about)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// List notifications.
    Inbox {
        /// Include notifications already marked as read.
        #[arg(long)]
        all: bool,
        /// Only notifications you're directly participating in.
        #[arg(long)]
        participating: bool,
        /// Results per page (max 50 for the inbox, 100 for a repo).
        #[arg(long, default_value_t = 50)]
        per_page: u8,
        /// List a single repository's notifications instead of the whole inbox ("owner/name").
        #[arg(long, value_name = "OWNER/NAME")]
        repo: Option<String>,
        /// Only notifications updated after this time (RFC3339, e.g. 2026-01-01T00:00:00Z).
        #[arg(long, value_name = "RFC3339")]
        since: Option<DateTime<Utc>>,
        /// Only notifications updated before this time (RFC3339).
        #[arg(long, value_name = "RFC3339")]
        before: Option<DateTime<Utc>>,
        /// Page number (1-based).
        #[arg(long)]
        page: Option<u32>,
    },
    /// Watch notifications as a live stream, optionally running a rules file per event.
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
        /// Run a command per event from this TOML rules file, instead of printing events.
        #[arg(long, value_name = "PATH")]
        rules: Option<PathBuf>,
    },
    /// Watch a repository (subscribe to notifications for all its activity).
    Subscribe {
        /// Repository as "owner/name".
        repo: String,
        /// Ignore the repository (suppress all notifications) instead of watching it.
        #[arg(long)]
        ignore: bool,
    },
    /// Stop watching or ignoring a repository (delete its subscription).
    Unsubscribe {
        /// Repository as "owner/name".
        repo: String,
    },
    /// Show your subscription status for a repository.
    Subscription {
        /// Repository as "owner/name".
        repo: String,
    },
    /// List the repositories you watch (your subscriptions).
    Subscriptions,
    /// Mark notifications as read (the whole inbox, or one repository with --repo).
    MarkRead {
        /// Mark only this repository's notifications ("owner/name").
        #[arg(long, value_name = "OWNER/NAME")]
        repo: Option<String>,
    },
    /// Operate on a single notification thread by id.
    Thread {
        #[command(subcommand)]
        action: ThreadCommand,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum ThreadCommand {
    /// Show the thread.
    Show {
        /// Thread id.
        id: String,
    },
    /// Mark the thread as read.
    Read {
        /// Thread id.
        id: String,
    },
    /// Mark the thread as done (remove it from the inbox).
    Done {
        /// Thread id.
        id: String,
    },
    /// Subscribe to the thread (or ignore it with --ignore).
    Subscribe {
        /// Thread id.
        id: String,
        /// Ignore the thread (suppress its notifications) instead of subscribing.
        #[arg(long)]
        ignore: bool,
    },
    /// Delete the thread subscription (mute it until you participate again).
    Unsubscribe {
        /// Thread id.
        id: String,
    },
    /// Show your subscription status for the thread.
    Subscription {
        /// Thread id.
        id: String,
    },
}

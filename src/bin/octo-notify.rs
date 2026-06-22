//! The `octo-notify` command-line tool, built behind the `cli` feature:
//!
//! ```text
//! cargo install octo-notify --features cli
//! GITHUB_TOKEN=$(gh auth token) octo-notify inbox --all
//! GITHUB_TOKEN=$(gh auth token) octo-notify watch --state ~/.cache/octo-notify.json
//! GITHUB_TOKEN=$(gh auth token) octo-notify subscribe octocat/hello-world
//! ```

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use futures::StreamExt;
use octo_notify::app::{Event, JsonFileStore};
use octo_notify::{Auth, Client, Listing, Notification};
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
    /// Watch notifications and run a command per event, per a TOML rules file.
    Dispatch {
        /// Path to the TOML rules file.
        #[arg(long, value_name = "PATH")]
        config: PathBuf,
        /// Minimum seconds between polls (the server may request longer).
        #[arg(long, default_value_t = 60)]
        interval: u64,
        /// Only notifications you're directly participating in.
        #[arg(long)]
        participating: bool,
        /// Include notifications already marked as read.
        #[arg(long)]
        all: bool,
        /// Process the current inbox once on start, instead of only new activity.
        #[arg(long)]
        show_existing: bool,
        /// Persist dedupe state so restarts resume without re-firing.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,
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
    /// Operate on a single notification thread by id.
    Thread {
        #[command(subcommand)]
        action: ThreadCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ThreadCommand {
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
        Command::Dispatch {
            config,
            interval,
            participating,
            all,
            show_existing,
            state,
        } => {
            dispatch(
                client,
                config,
                interval,
                participating,
                all,
                show_existing,
                state,
            )
            .await
        }
        Command::Subscribe { repo, ignore } => subscribe(&client, &repo, ignore).await,
        Command::Unsubscribe { repo } => unsubscribe(&client, &repo).await,
        Command::Subscription { repo } => subscription_status(&client, &repo).await,
        Command::Thread { action } => thread_command(&client, action).await,
    }
}

async fn thread_command(client: &Client, action: ThreadCommand) -> anyhow::Result<()> {
    match action {
        ThreadCommand::Show { id } => {
            let n = client.thread(id.as_str()).get().await?;
            let flag = if n.unread { "●" } else { "○" };
            println!(
                "{flag} [{}] {} - {} ({})",
                n.reason, n.repository.full_name, n.subject.title, n.subject.kind,
            );
            Ok(())
        }
        ThreadCommand::Read { id } => {
            client.thread(id.as_str()).mark_read().await?;
            println!("thread {id}: read");
            Ok(())
        }
        ThreadCommand::Done { id } => {
            client.thread(id.as_str()).mark_done().await?;
            println!("thread {id}: done");
            Ok(())
        }
        ThreadCommand::Subscribe { id, ignore } => {
            let sub = client.thread(id.as_str()).set_subscription(ignore).await?;
            println!(
                "thread {id}: {}",
                thread_subscription_state(sub.subscribed, sub.ignored)
            );
            Ok(())
        }
        ThreadCommand::Unsubscribe { id } => {
            client.thread(id.as_str()).delete_subscription().await?;
            println!("thread {id}: unsubscribed");
            Ok(())
        }
        ThreadCommand::Subscription { id } => match client.thread(id.as_str()).subscription().await
        {
            Ok(sub) => {
                println!(
                    "thread {id}: {}",
                    thread_subscription_state(sub.subscribed, sub.ignored)
                );
                Ok(())
            }
            // 404 means no subscription record exists for this thread.
            Err(octo_notify::Error::Api { status, .. }) if status.as_u16() == 404 => {
                println!("thread {id}: not subscribed");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
    }
}

fn thread_subscription_state(subscribed: bool, ignored: bool) -> &'static str {
    if ignored {
        "ignored"
    } else if subscribed {
        "subscribed"
    } else {
        "not subscribed"
    }
}

/// Split an `owner/name` repository argument, rejecting anything malformed.
fn split_repo(repo: &str) -> anyhow::Result<(&str, &str)> {
    match repo.split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok((owner, name))
        }
        _ => anyhow::bail!("expected repository as \"owner/name\", got {repo:?}"),
    }
}

async fn subscribe(client: &Client, repo: &str, ignore: bool) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    let handler = client.repo(owner, name);
    let sub = if ignore {
        handler.ignore().await?
    } else {
        handler.subscribe().await?
    };
    println!(
        "{repo}: {}",
        subscription_state(sub.subscribed, sub.ignored)
    );
    Ok(())
}

async fn unsubscribe(client: &Client, repo: &str) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    client.repo(owner, name).delete_subscription().await?;
    println!("{repo}: unsubscribed");
    Ok(())
}

async fn subscription_status(client: &Client, repo: &str) -> anyhow::Result<()> {
    let (owner, name) = split_repo(repo)?;
    match client.repo(owner, name).subscription().await {
        Ok(sub) => {
            println!(
                "{repo}: {}",
                subscription_state(sub.subscribed, sub.ignored)
            );
            Ok(())
        }
        // 404 is the API's way of saying "no subscription exists" for this repo.
        Err(octo_notify::Error::Api { status, .. }) if status.as_u16() == 404 => {
            println!("{repo}: not subscribed");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn subscription_state(subscribed: bool, ignored: bool) -> &'static str {
    if ignored {
        "ignored"
    } else if subscribed {
        "watching"
    } else {
        "not subscribed"
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

/// A dispatch rules file: `match` mode plus a list of `[[rule]]`s.
#[derive(serde::Deserialize, Default)]
struct DispatchConfig {
    #[serde(rename = "match", default)]
    match_mode: MatchMode,
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(serde::Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum MatchMode {
    /// Run only the first matching rule.
    #[default]
    First,
    /// Run every matching rule.
    All,
}

#[derive(serde::Deserialize)]
struct Rule {
    reason: Option<String>,
    subject_type: Option<String>,
    repo: Option<String>,
    run: String,
    mark: Option<MarkAction>,
}

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum MarkAction {
    Read,
    Done,
}

/// A rule matches when each of its set matchers matches (omitted matchers match anything).
fn rule_matches(rule: &Rule, n: &Notification) -> bool {
    rule.reason
        .as_deref()
        .is_none_or(|r| r == n.reason.as_str())
        && rule
            .subject_type
            .as_deref()
            .is_none_or(|t| t == n.subject.kind.as_str())
        && rule
            .repo
            .as_deref()
            .is_none_or(|r| r == n.repository.full_name)
}

fn subject_url(n: &Notification) -> &str {
    n.subject.url.as_ref().map(|u| u.as_str()).unwrap_or("")
}

fn render(template: &str, n: &Notification) -> String {
    template
        .replace("{repo}", &n.repository.full_name)
        .replace("{thread_id}", n.id.as_str())
        .replace("{title}", &n.subject.title)
        .replace("{url}", subject_url(n))
        .replace("{reason}", n.reason.as_str())
        .replace("{type}", n.subject.kind.as_str())
}

async fn run_command(cmd: &str, n: &Notification) -> std::io::Result<std::process::ExitStatus> {
    let mut command = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    command
        .env("OCTO_REPO", &n.repository.full_name)
        .env("OCTO_THREAD_ID", n.id.as_str())
        .env("OCTO_TITLE", &n.subject.title)
        .env("OCTO_URL", subject_url(n))
        .env("OCTO_REASON", n.reason.as_str())
        .env("OCTO_TYPE", n.subject.kind.as_str());
    command.status().await
}

async fn dispatch(
    client: Client,
    config: PathBuf,
    interval: u64,
    participating: bool,
    all: bool,
    show_existing: bool,
    state: Option<PathBuf>,
) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&config)?;
    let cfg: DispatchConfig = toml::from_str(&text)?;

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
        Some(path) => builder.store(JsonFileStore::open(path)?),
        None => builder,
    };
    let poller = builder.build();

    eprintln!(
        "dispatching ({} rule(s); interval >= {interval}s); Ctrl-C to stop",
        cfg.rule.len()
    );
    let mut events = Box::pin(poller.stream());
    while let Some(event) = events.next().await {
        let n = match event {
            Ok(e) => e.into_notification(),
            Err(e) => {
                eprintln!("error: {e}");
                continue;
            }
        };
        let mut matching = cfg.rule.iter().filter(|r| rule_matches(r, &n));
        let selected: Vec<&Rule> = match cfg.match_mode {
            MatchMode::First => matching.next().into_iter().collect(),
            MatchMode::All => matching.collect(),
        };
        for rule in selected {
            let rendered = render(&rule.run, &n);
            match run_command(&rendered, &n).await {
                Ok(status) if status.success() => {
                    if let Some(mark) = rule.mark {
                        let result = match mark {
                            MarkAction::Read => client.thread(n.id.clone()).mark_read().await,
                            MarkAction::Done => client.thread(n.id.clone()).mark_done().await,
                        };
                        if let Err(e) = result {
                            eprintln!("mark failed for {}: {e}", n.id);
                        }
                    }
                }
                Ok(status) => eprintln!("command exited ({status}) for {}", n.id),
                Err(e) => eprintln!("command failed for {}: {e}", n.id),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Notification {
        serde_json::from_str(
            r#"{
                "id": "1", "unread": true, "reason": "mention",
                "updated_at": "2024-05-01T10:00:00Z", "last_read_at": null,
                "subject": {"title":"Hello","url":"https://api.github.com/repos/octocat/hello/issues/1","latest_comment_url":null,"type":"Issue"},
                "repository": {"id":1,"name":"hello","full_name":"octocat/hello","private":false,"fork":false,"html_url":"https://github.com/octocat/hello","owner":{"login":"octocat","id":1,"html_url":"https://github.com/octocat","type":"User"}},
                "url":"https://api.github.com/notifications/threads/1",
                "subscription_url":"https://api.github.com/notifications/threads/1/subscription"
            }"#,
        )
        .unwrap()
    }

    fn rule(reason: Option<&str>, subject_type: Option<&str>, repo: Option<&str>) -> Rule {
        Rule {
            reason: reason.map(String::from),
            subject_type: subject_type.map(String::from),
            repo: repo.map(String::from),
            run: String::new(),
            mark: None,
        }
    }

    #[test]
    fn matchers_are_anded_and_omitted_match_all() {
        let n = sample();
        assert!(rule_matches(&rule(None, None, None), &n));
        assert!(rule_matches(
            &rule(Some("mention"), Some("Issue"), Some("octocat/hello")),
            &n
        ));
        assert!(!rule_matches(&rule(Some("author"), None, None), &n));
        assert!(!rule_matches(&rule(None, Some("PullRequest"), None), &n));
    }

    #[test]
    fn render_substitutes_placeholders() {
        let n = sample();
        assert_eq!(
            render("{reason} {repo} {type} #{thread_id}", &n),
            "mention octocat/hello Issue #1"
        );
    }

    #[test]
    fn split_repo_accepts_owner_name_and_rejects_the_rest() {
        assert_eq!(split_repo("octocat/hello").unwrap(), ("octocat", "hello"));
        for bad in ["octocat", "octocat/", "/hello", "a/b/c", ""] {
            assert!(split_repo(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn subscription_state_labels() {
        assert_eq!(subscription_state(true, false), "watching");
        assert_eq!(subscription_state(false, true), "ignored");
        assert_eq!(subscription_state(true, true), "ignored");
        assert_eq!(subscription_state(false, false), "not subscribed");
    }

    #[test]
    fn thread_subscription_state_labels() {
        assert_eq!(thread_subscription_state(true, false), "subscribed");
        assert_eq!(thread_subscription_state(false, true), "ignored");
        assert_eq!(thread_subscription_state(true, true), "ignored");
        assert_eq!(thread_subscription_state(false, false), "not subscribed");
    }
}

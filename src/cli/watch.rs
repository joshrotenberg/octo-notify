//! The `watch` command: stream notifications, and with `--rules` run a command per event.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use anstyle::{AnsiColor, Style};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::app::{Event, JsonFileStore};
use crate::{Client, Notification};

pub(crate) struct WatchArgs {
    pub(crate) interval: u64,
    pub(crate) participating: bool,
    pub(crate) all: bool,
    pub(crate) show_existing: bool,
    pub(crate) state: Option<PathBuf>,
    pub(crate) rules: Option<PathBuf>,
}

pub(crate) async fn run(client: Client, args: WatchArgs) -> anyhow::Result<()> {
    // Load and parse the rules file up front so a bad config fails before we start polling.
    let rules = match &args.rules {
        Some(path) => {
            let text = std::fs::read_to_string(path)?;
            Some(toml::from_str::<RulesConfig>(&text)?)
        }
        None => None,
    };

    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down at next tick...");
        shutdown.cancel();
    });

    let builder = client
        .poller()
        .min_interval(Duration::from_secs(args.interval))
        .participating_only(args.participating)
        .include_read(args.all)
        .emit_existing_on_start(args.show_existing)
        .cancellation(cancel);
    let builder = match args.state {
        Some(path) => {
            eprintln!("persisting state to {}", path.display());
            builder.store(JsonFileStore::open(path)?)
        }
        None => builder,
    };
    let poller = builder.build();

    let interval = args.interval;
    let mut events = Box::pin(poller.stream());
    match rules {
        // With a rules file, run a command per event (the former `dispatch`); rules own the output.
        Some(cfg) => {
            eprintln!(
                "watching with {} rule(s) (interval >= {interval}s); Ctrl-C to stop",
                cfg.rule.len()
            );
            while let Some(event) = events.next().await {
                let n = match event {
                    Ok(e) => e.into_notification(),
                    Err(e) => {
                        eprintln!("error: {e}");
                        continue;
                    }
                };
                run_rules(&client, &cfg, &n).await;
            }
        }
        // Without rules, print each event.
        None => {
            let color = std::io::stdout().is_terminal() && !no_color();
            let err_color = std::io::stderr().is_terminal() && !no_color();
            eprintln!("watching (interval >= {interval}s); Ctrl-C to stop");
            while let Some(event) = events.next().await {
                match event {
                    Ok(Event::New(n)) => println!("{}", format_event(EventKind::New, &n, color)),
                    Ok(Event::Updated(n)) => {
                        println!("{}", format_event(EventKind::Updated, &n, color))
                    }
                    Err(e) => eprintln!(
                        "{}",
                        paint(
                            AnsiColor::Red.on_default(),
                            &format!("error: {e}"),
                            err_color
                        )
                    ),
                }
            }
        }
    }
    Ok(())
}

/// A rules file: `match` mode plus a list of `[[rule]]`s.
#[derive(serde::Deserialize, Default)]
struct RulesConfig {
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

/// Run the matching rules for one notification: execute each `run` command and, on a zero exit,
/// apply that rule's optional `mark`.
async fn run_rules(client: &Client, cfg: &RulesConfig, n: &Notification) {
    let mut matching = cfg.rule.iter().filter(|r| rule_matches(r, n));
    let selected: Vec<&Rule> = match cfg.match_mode {
        MatchMode::First => matching.next().into_iter().collect(),
        MatchMode::All => matching.collect(),
    };
    for rule in selected {
        let rendered = render(&rule.run, n);
        match run_command(&rendered, n).await {
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

#[derive(Clone, Copy)]
enum EventKind {
    New,
    Updated,
}

/// Honor the `NO_COLOR` convention (https://no-color.org): any non-empty value disables color.
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

/// Wrap `text` in `style`'s ANSI codes when `color` is set, otherwise return it as-is.
fn paint(style: Style, text: &str, color: bool) -> String {
    if color {
        format!("{}{text}{}", style.render(), style.render_reset())
    } else {
        text.to_string()
    }
}

/// Render one poller event as a single line, colorized when `color` is set.
fn format_event(kind: EventKind, n: &Notification, color: bool) -> String {
    let (glyph, label, label_style) = match kind {
        EventKind::New => ('●', "NEW", AnsiColor::Green.on_default().bold()),
        EventKind::Updated => ('↻', "UPD", AnsiColor::Yellow.on_default().bold()),
    };
    let head = paint(label_style, &format!("{glyph} {label}"), color);
    let reason = paint(
        AnsiColor::Cyan.on_default(),
        &format!("{:<13}", n.reason.as_str()),
        color,
    );
    let repo = paint(Style::new().bold(), &n.repository.full_name, color);
    let kind_tag = paint(
        Style::new().dimmed(),
        &format!("({})", n.subject.kind),
        color,
    );
    format!("{head} {reason} {repo}  {}  {kind_tag}", n.subject.title)
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
    fn format_event_plain_has_fields_and_no_ansi() {
        let line = format_event(EventKind::New, &sample(), false);
        assert!(line.contains("NEW"));
        assert!(line.contains("mention"));
        assert!(line.contains("octocat/hello"));
        assert!(line.contains("Hello"));
        assert!(line.contains("(Issue)"));
        assert!(
            !line.contains('\u{1b}'),
            "plain output must not contain ANSI escapes"
        );
    }

    #[test]
    fn format_event_colored_emits_ansi() {
        let line = format_event(EventKind::Updated, &sample(), true);
        assert!(
            line.contains('\u{1b}'),
            "colored output should contain ANSI escapes"
        );
        assert!(line.contains("UPD"));
        assert!(line.contains("Hello"));
    }
}

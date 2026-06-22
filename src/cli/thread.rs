//! The `thread` subcommand group: operate on a single notification thread by id.

use super::args::ThreadCommand;
use crate::Client;

pub(crate) async fn run(client: &Client, action: ThreadCommand) -> anyhow::Result<()> {
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
            println!("thread {id}: {}", state(sub.subscribed, sub.ignored));
            Ok(())
        }
        ThreadCommand::Unsubscribe { id } => {
            client.thread(id.as_str()).delete_subscription().await?;
            println!("thread {id}: unsubscribed");
            Ok(())
        }
        ThreadCommand::Subscription { id } => {
            match client.thread(id.as_str()).subscription().await {
                Ok(sub) => {
                    println!("thread {id}: {}", state(sub.subscribed, sub.ignored));
                    Ok(())
                }
                // 404 means no subscription record exists for this thread.
                Err(crate::Error::Api { status, .. }) if status.as_u16() == 404 => {
                    println!("thread {id}: not subscribed");
                    Ok(())
                }
                Err(e) => Err(e.into()),
            }
        }
    }
}

fn state(subscribed: bool, ignored: bool) -> &'static str {
    if ignored {
        "ignored"
    } else if subscribed {
        "subscribed"
    } else {
        "not subscribed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_labels() {
        assert_eq!(state(true, false), "subscribed");
        assert_eq!(state(false, true), "ignored");
        assert_eq!(state(true, true), "ignored");
        assert_eq!(state(false, false), "not subscribed");
    }
}

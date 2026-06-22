//! The `subscriptions` command: list the repositories you watch.

use crate::Client;

pub(crate) async fn run(client: &Client) -> anyhow::Result<()> {
    let repos = client.subscriptions().all().await?;
    println!(
        "{} watched repositor{}\n",
        repos.len(),
        if repos.len() == 1 { "y" } else { "ies" },
    );
    for r in &repos {
        let mut tags = Vec::new();
        if r.private {
            tags.push("private");
        }
        if r.fork {
            tags.push("fork");
        }
        if tags.is_empty() {
            println!("{}", r.full_name);
        } else {
            println!("{} ({})", r.full_name, tags.join(", "));
        }
    }
    Ok(())
}

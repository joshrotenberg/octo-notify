//! The `octo-notify` command-line tool, a thin shim over [`octo_notify::cli`].
//!
//! All command logic lives in the library's `cli` module (behind the `cli` feature); this
//! binary only forwards to it.
//!
//! ```text
//! cargo install octo-notify --features cli
//! GITHUB_TOKEN=$(gh auth token) octo-notify inbox --all
//! GITHUB_TOKEN=$(gh auth token) octo-notify watch --rules rules.toml
//! ```

fn main() -> anyhow::Result<()> {
    octo_notify::cli::run()
}

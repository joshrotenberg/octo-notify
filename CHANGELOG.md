# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-06-22

### Added

- Repository and thread subscription in the library and CLI ([#46](https://github.com/joshrotenberg/octo-notify/pull/46))
- List watched repositories (GET /user/subscriptions) ([#51](https://github.com/joshrotenberg/octo-notify/pull/51))
- *(cli)* Mark-read command and repo/time/page filters on inbox ([#52](https://github.com/joshrotenberg/octo-notify/pull/52))
- *(cli)* Collapse dispatch into watch --rules ([#55](https://github.com/joshrotenberg/octo-notify/pull/55)) [**breaking**]

### Changed

- *(cli)* Split the CLI into src/cli/ modules ([#56](https://github.com/joshrotenberg/octo-notify/pull/56))

### Documentation

- Expand dispatch example config and move to repo root ([#45](https://github.com/joshrotenberg/octo-notify/pull/45))
- Refresh README status and version for the 0.3.0 release ([#60](https://github.com/joshrotenberg/octo-notify/pull/60))



## [0.2.0] - 2026-06-21

### Added

- *(examples)* Add --state to the watch example ([#30](https://github.com/joshrotenberg/octo-notify/pull/30))
- Opt-in PollConfig::prune_after to auto-prune seen state ([#32](https://github.com/joshrotenberg/octo-notify/pull/32))
- Fetch_subject to resolve a notification's subject JSON ([#36](https://github.com/joshrotenberg/octo-notify/pull/36))
- Tracing instrumentation behind a tracing feature ([#37](https://github.com/joshrotenberg/octo-notify/pull/37))
- Reactive token refresh on 401 ([#38](https://github.com/joshrotenberg/octo-notify/pull/38))
- SqliteStore for persistent dedupe state ([#40](https://github.com/joshrotenberg/octo-notify/pull/40))
- Ship an octo-notify CLI behind a cli feature ([#43](https://github.com/joshrotenberg/octo-notify/pull/43))
- Octo-notify dispatch - run commands from notifications ([#44](https://github.com/joshrotenberg/octo-notify/pull/44))

### Documentation

- Add item-level rustdoc examples ([#42](https://github.com/joshrotenberg/octo-notify/pull/42))

### Testing

- GitHub Enterprise Server base-path coverage ([#41](https://github.com/joshrotenberg/octo-notify/pull/41))



## [0.1.0] - 2026-06-20

### Added

- Scaffold notifications client with GET /notifications
- Complete endpoint coverage and pagination
- Add the polling engine (poller, stream, state, filters)
- Add JsonFileStore for cross-restart dedupe ([#9](https://github.com/joshrotenberg/octo-notify/pull/9))
- Bulk mark_read_each / mark_done_each ([#11](https://github.com/joshrotenberg/octo-notify/pull/11))
- Refreshing TokenProvider for expiring credentials ([#14](https://github.com/joshrotenberg/octo-notify/pull/14))
- Optional RetryPolicy for one-shot calls ([#15](https://github.com/joshrotenberg/octo-notify/pull/15))

### Documentation

- Tighten README scope section and octocrab comparison ([#3](https://github.com/joshrotenberg/octo-notify/pull/3))
- Factual scrub and currency pass ([#16](https://github.com/joshrotenberg/octo-notify/pull/16))
- Expand lib.rs into a full guide and remove SPEC.md ([#29](https://github.com/joshrotenberg/octo-notify/pull/29))

### Miscellaneous

- Set up CI, release-plz, and project docs
- *(deps)* Bump actions/checkout ([#1](https://github.com/joshrotenberg/octo-notify/pull/1))



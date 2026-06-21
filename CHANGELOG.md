# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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



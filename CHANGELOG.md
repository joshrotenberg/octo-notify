# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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



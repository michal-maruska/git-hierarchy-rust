# Changelog

All user-visible changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

Every pull request (PR) or user-visible commit must append its user-visible changes to the `[CURRENT]` section below. When a new release is prepared, the release script (`ci/bump-release.sh`) converts `[CURRENT]` into a versioned section (`[<version>] - <date>`) and initializes a new empty `[CURRENT]` section for subsequent development.

## [CURRENT]

### Breaking Changes
- Changed CLI flag for specifying git repository directory from `-D` to `-g` across commands (`git-segment`, `git-sum`, `git-rebase-poset`, `git-rebase-segment`, `git-walk-down`, `gitk-poset`) (PR #33).

### New Features
- Introduced `gitk-poset` tool to launch `gitk` visualizing the tops and bases of the hierarchy poset.

### Fixes and Improvements
- Propagated repository and CLI errors using `Result` and `?` instead of panicking.
- Validated segment, summand, and reference names to reject leading hyphens (`-`) and prevent option injection.
- Added comprehensive integration test coverage for CLI binaries and rebase error states.

## [0.1.0] - 2025-02-17
- Initial release of `git-hierarchy` suite (`git-walk-down`, `git-rebase-poset`, `git-rebase-segment`, `git-segment`, `git-sum`).

## Another reimplementation of git-hierarchy, in Rust

See
- https://github.com/MichalMaruska/git-hierarchy
- https://github.com/MichalMaruska/git-hierarchy-go


## Difference:

More parts might be done natively, instead of invoking `git'
More care is necessary to sync the state after such invocations.


## Documenting Changes & Releases

User-visible changes are tracked in `CHANGELOG.md` following the [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) standard format:

* Every pull request (PR) or commit with user-visible changes should document them under the `## [CURRENT]` section in `CHANGELOG.md` (under `### Breaking Changes`, `### New Features`, or `### Fixes and Improvements`).
* When cutting a release, run `./ci/bump-release.sh [VERSION]` (or `./ci/bump-release.sh` to use the version from `Cargo.toml`). This converts `## [CURRENT]` into `## [<version>] - <date>` and creates a new empty `## [CURRENT]` section for subsequent PRs.


## todo:
might try using git2 with "vendored-libgit2"


## learnt about Rust/git-rs:

2-step downcasting from a trait object:
* Any ... type-erasure?  as_any produces Any.... and that has vtable, which...
* from Any ....allows to downcast<>


OnceCell

*
Cannot pass Reference:

move occurs because `segment._start` has type `git2::Reference<'_>`, which does not implement the `Copy` trait
cannot move out of `segment._start` which is behind a shared reference

# Introduction

How to develop several features at once against a moving upstream: keep every
feature line dependent on exactly what it builds on, rebase the whole set in one
go, and drop a layer the moment upstream adopts it.

git-hierarchy does not change how you commit or what git stores. It only records,
for each feature branch, **what it sits on** and **where your own work starts** --
two facts plain git throws away. With those two facts, keeping a stack of
features in sync with upstream is a command, not a manual archaeology session.

## The problem it solves

While `main` moves, you have `feat-a`, `feat-b` built on top of `feat-a`, and an
integration branch merging both. Two everyday operations both hurt:

- `git merge main` into each feature puts upstream's history *inside* your
  feature branch: merge commits in review, tangled graphs, conflicts that
  reappear on every sync.
- `git rebase` gives linear history, but you must know the base of every branch
  and replay them in dependency order yourself -- and when a feature is adopted
  upstream, the whole stack has to be reshuffled by hand.

The information git lacks is exactly: *which commits are mine?* and *what am I
applied on?* It is not in the graph, so every tool has to guess. git-hierarchy
makes you state it once, then uses it.

## The model

Two kinds of objects, both ordinary branches plus a few side references:

- **Segment** -- a branch with provenance.
  - `refs/heads/<name>` -- the work.
  - `refs/base/<name>` -- a *symbolic* reference naming what the segment sits on:
    a branch, a **tag**, or another segment's branch.
  - `refs/start/<name>` -- the commit where *your* work begins. Everything from
    `start` to the branch tip is yours; everything before it belongs to the base.
- **Sum** -- a merge (or octopus) commit plus the list of what it combines.
  - `refs/heads/<name>` -- the merge commit.
  - `refs/sums/<name>/<i>` -- symbolic references to the components. Components
    may be segments **or other sums**, so the hierarchy is a poset, not just a
    stack.

The hierarchy is discovered by walking `refs/base` and `refs/sums` from any root.
Segments and sums are consistently shown with a status:

| status | meaning |
|---|---|
| `up-to-date` | the base has not moved past `start`; nothing to do |
| `need-rebase` | segment: the base moved; rebase will replay only your commits |
| `upgraded` | sum: its summands only moved forward; the merge still accounts for everything |
| `needs update` | sum: the hierarchy around it no longer matches |
| `not a merge (moved ahead?)` | the sum branch tip is not a merge commit any more |

"Clean" throughout this document means `up-to-date`.

## The daily loop

Create a segment (the branch plus the two records; it checks it out):

```console
$ git segment create feat-a main
should checkout now refs/heads/feat-a
```

Commit as usual. When upstream moves, ask:

```console
$ git walk-down feat-a
segment feat-a: on refs/heads/main	need-rebase
4d99f79...: feat-a: work
```

and bring it up to date:

```console
$ git rebase-poset feat-a
$ git walk-down feat-a
segment feat-a: on refs/heads/main	up-to-date
8a49e39...: feat-a: work
```

`git rebase-poset` replays exactly `start..head` -- only your commits -- onto the
new base and moves `start` forward. The feature branch ends up looking as if it
had been written on today's `main`; upstream's history never enters it, and the
commits that landed upstream in the meantime are not replayed a second time.

## Stacking features

A segment's base may be another segment:

```console
$ git segment create feat-b feat-a
```

`git rebase-poset feat-b` understands the chain: it rebases `feat-a` first and
`feat-b` after it (dependencies first, the reverse of the discovery order), so
one command keeps the whole stack on today's upstream. Sums join segments:

```console
$ git checkout --detach feat-a
$ git merge feat-b -m "integrate feat-b"
$ git sum define combined feat-a feat-b -H "$(git rev-parse HEAD)"
$ git walk-down combined
sum combined	up-to-date
segment feat-a: on refs/heads/main	up-to-date
segment feat-b: on refs/heads/main	up-to-date
```

After upstream moves, `git rebase-poset combined` refreshes everything at once:
the segments are rebased, then the sum is re-merged (`Sum: combined`). The
integration branch is rebuilt from today's components instead of carrying a
stale merge forward.

## Tags as upstream anchors

A base may be a tag, which makes "work based on a release" first class:

```console
$ git segment create feat-a v1.0
```

When the next release exists, point the segment at it and rebase:

```console
$ git segment update feat-a v2.0
$ git rebase-poset feat-a
```

Only your commits move; the tag is never rewritten.

## Adopting upstream progress, and simplifying

The interesting case: upstream absorbs `feat-a`, so the layer no longer needs to
exist locally. Point its dependents at what they now effectively sit on, and
rebase:

```console
$ git segment update feat-b main
$ git rebase-poset feat-b
$ git segment delete feat-a
```

Only `feat-b`'s own work is replayed directly onto `main` (`start` marks its
boundary), the adopted commits are no longer carried, and `feat-a`'s bookkeeping
goes away. The stack got shorter and nothing was merged.

For a bulk change across a whole hierarchy, `git walk-down -r <FROM> <TO> <root>`
re-points every base reference in the tree in one pass; `git delete-down <root>`
deletes a hierarchy in a safe order (sums and dependents before the bases they
stand on).

This is the core simplification loop: **as upstream advances, the local hierarchy
only shrinks.** Each rebase keeps the remaining layers aligned; each adoption
removes a layer.

## Moving a hierarchy around

The complete state (bases, starts, sums) can be serialized -- dependency-first --
into an annotated tag's message, a branch file, or stdout:

```console
$ git store-hierarchy -t snapshot -c combined -m "integration state" combined
$ git restore-hierarchy -t snapshot --stdout
# Git Hierarchy Store
SEGMENT feat-b base=refs/heads/main start=... head=...
SEGMENT feat-a base=refs/heads/main start=... head=...
SUM combined head=... summands=refs/heads/feat-a,refs/heads/feat-b
```

and restored elsewhere:

```console
$ git restore-hierarchy -t snapshot            # recreate the references
$ git restore-hierarchy -t snapshot --dry-run  # show the plan only
```

Because the serialization is in dependency-first order, restore just applies it
in order -- bases before the segments standing on them. This is how the setup
travels: back it up, hand it to a teammate, recreate it in CI, or rebuild it
after deleting the local hierarchy entirely.

## Command reference

| command | purpose |
|---|---|
| `git segment create <name> <base>` | define a segment and check it out |
| `git segment define <name> <base>` | define without checking out |
| `git segment update <name> <new-base>` | move an existing segment to another base |
| `git segment list` | list segments with status |
| `git rebase-poset <root>` | rebase the hierarchy, dependencies first |
| `git walk-down [root]` | inspect the hierarchy; `-r FROM TO` rebinds bases; `-c SUFFIX` clones it |
| `git delete-down <root>` | delete a hierarchy, dependents first |
| `git sum define <name> <components>... -H <merge>` | record a merge as a sum |
| `git sum list` / `git sum show <name>` | inspect sums |
| `git store-hierarchy` | serialize to tag (`-t`), branch (`-b`), file (`-f`), stdout |
| `git restore-hierarchy` | restore from tag/branch/file/stdin; `-n` dry run |
| `git rebase-segment <name>` | rebase a single segment |

All commands take `-g <dir>` to name the repository and are installed as
`git-<name>` so they read naturally as `git <name>`.

## Building

```console
$ cargo build --release
```

The binaries land in `target/release/`; put that directory on your `PATH`.
`build.rs` creates the `git-delete-down` symlink next to `git-walk-down`, so
both names reach the same multi-call binary.

See [description.md](description.md) for how the hierarchy graph is discovered,
and [readme.md](readme.md) for the project's background.

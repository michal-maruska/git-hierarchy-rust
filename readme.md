## Another reimplementation of git-hierarchy, in Rust

See
- https://github.com/MichalMaruska/git-hierarchy
- https://github.com/MichalMaruska/git-hierarchy-go


## Difference:

We invoke `git' only for octopus merge (more than 2 summands, conflicts).

More care is necessary to sync the state after such invocations.


## todo:
might try using git2 with "vendored-libgit2"

see [design document](description.md) for how the hiearchy graph is created.

#![allow(static_mut_refs)]
#![deny(elided_lifetimes_in_paths)]

use crate::utils::{concatenate,extract_name};
use git2::{Branch, BranchType, Commit, Oid,
           Error,
           Reference, Repository, build::CheckoutBuilder,
           Remote,
           ReferenceFormat,
           Sort,
           StatusShow,StatusOptions, Statuses,};
#[allow(unused)]
use tracing::{debug, info, warn};
use std::path::PathBuf;

// this consults the store.
pub fn git_same_ref(
    repository: &Repository,
    reference: &Reference<'_>,
    next: &Reference<'_>,
) -> bool {
    fn sha<'a>(_repository: &'a Repository, reference: &Reference<'a>) -> Oid {
        let direct = reference.resolve().unwrap();
        let oid = direct.target().unwrap();
        debug!("git_same_ref: {:?} {:?}",
               reference.name().unwrap(),
               oid);
        oid
    }

    sha(repository, reference) == sha(repository, next)
}

// ancestor <---is parent-- ........ descendant
// fixme: is this wrong?   Initial........descendant ......ancestor   would say true, but it's not!
pub fn is_linear_ancestor(repository: &Repository, ancestor: Oid, descendant: Oid) -> Result<bool,git2::Error>
{
    debug!("is_linear_ancestor: {} ---parent---> {}", descendant, ancestor);
    if ancestor == descendant { return Ok(true);}

    let mut walk = repository.revwalk()?;
    walk.push(descendant)?; // .expect("should set upper bound for Walk");
    // segment.reference.borrow().target().unwrap()

    // mmc: maybe hide the parent of the ancestor.
    // walk.hide(ancestor)?; // .expect("should set the lower bound for Walk");

    walk.set_sorting(Sort::TOPOLOGICAL)?; // .expect("should set the topo ordering of the Walk");

    if walk.next().is_none() {
        return Ok(false);
    }

    for oid in walk {
        // None at the end?
        if let Ok(oid) = oid {
            if oid == ancestor {
                return Ok(true);
            }
            // slow?
            if repository.find_commit(oid)?.parent_count() > 1 {
                debug!("is_linear_ancestor: merge commit found");
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }
    Ok(true)
}


/// Searches the reflog of `ref_name` for entries whose new OID matches `target`.
/// Returns the index into the reflog (0 = most recent entry) of the first match, if any.
pub fn find_commit_in_reflog(
    repo: &Repository,
    ref_name: &str,
    target: Oid,
) -> Result<Option<usize>, git2::Error> {
    let reflog = repo.reflog(ref_name)?;

    for (i, entry) in reflog.iter().enumerate() {
        if entry.id_new() == target {
            return Ok(Some(i));
        }
    }

    Ok(None)
}


pub const GIT_HEADS_PATTERN: &str = "refs/heads/";

/// alternative to:
/// `git checkout name -b target`
/// git_run(repository, &["checkout", "--no-track", "-B", temp_head, new_start.name().unwrap()]);
pub fn checkout_new_head_at<'repo>(
    repository: &'repo Repository,
    name: Option<&'_ str>,
    target: &Commit<'_>,
) -> Option<Branch<'repo>> {
    // reflog?

    // https://libgit2.org/docs/reference/main/checkout/git_checkout_head.html
    // error: temporary value is freed at the end of this statement
    let tree = target.tree().unwrap();

    let mut checkout_opts = CheckoutBuilder::new();
    checkout_opts.safe();
    checkout_opts.force();

    repository
        .checkout_tree(tree.as_object(), Some(&mut checkout_opts))
        .expect("failed to checkout the newly created branch");

    if let Some(name) = name {
        info!("create temp branch {:?}", name);

        // target = target.peel_to_commit().unwrap()
        let new_branch = repository.branch(name, target, false).unwrap();

        let full_name = new_branch.name().unwrap().unwrap();
        let full_name = concatenate(GIT_HEADS_PATTERN, full_name);
        info!("checkout {:?} to {:?}", full_name, target);

        repository
            .set_head(&full_name)
            .expect("failed to create a branch on given commit");
        Some(new_branch)
    } else {
        info!("detached checkout {:?}", target.id());
        repository.set_head_detached(target.id()).unwrap();
        None
    }
}

// get the status: list of file modified in Index
pub fn staged_files<'repo>(repository: &'repo Repository) -> Result<Statuses<'repo>, Error>{
    let mut status_options = StatusOptions::new();
    status_options
        .show(StatusShow::Index)
        .include_unmodified(false) ;

    repository.statuses(Some(&mut status_options))
}

// todo: I need my error.  Result<Statuses<'_>, Error>
// why not repository.state() == RepositoryState::Clean
pub fn repository_clean(repository: &Repository) -> bool {
    // rely on
    let options = &mut StatusOptions::new();
        options.include_untracked(false)
        .include_ignored(false);
    let statuses = repository.statuses(Some(options)).unwrap();
    if ! statuses.is_empty() {
        eprintln!("repository is not clean: ");
        for entry in statuses.iter() {
            eprintln!("{:?}", entry.path());
        }
        return false;
    }
    true
}

pub fn open_repository(directory_option: Option<&PathBuf>) -> Result<Repository, Error> {
    if let Some(directory) = directory_option {
        Repository::open(directory)
    } else {
        Repository::open_from_env()
    }
}

pub fn upstream_of<'repo>(repository: &'repo Repository, branch: &Branch<'repo>) -> Option<(Remote<'repo>, Branch<'repo>, String)>
{
    let upstream = branch.upstream().ok()?;

    let upstream_name = upstream.name().unwrap().unwrap();
    let [rem, branch_name]= upstream_name.split('/').take(2).next_chunk().unwrap();

    let remote = repository.find_remote(rem).ok()?;
    // we have to drop branch_name, before ....returning .... @upstream ... because it's moving out.
    let name = branch_name.to_owned();
    Some((remote, upstream, name))
}


// &Reference -> Branch
pub fn to_branch<'repo>(repository: &'repo Repository, reference: &Reference<'repo>) -> Branch<'repo>
{
    // let b = Branch::wrap(*reference); // cannot move out of `*reference` which is behind a mutable reference
    let name = Reference::normalize_name(reference.name().unwrap(), ReferenceFormat::NORMAL).unwrap();

    repository
        .find_branch(extract_name(&name), BranchType::Local)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{create_commit, TestRepo};
    use std::fs;

    #[test]
    fn test_git_same_ref() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit1 = create_commit(repo, "commit 1", &[]);
        let commit2 = create_commit(repo, "commit 2", &[&commit1]);

        let b1 = repo.branch("b1", &commit1, false).unwrap();
        let b2 = repo.branch("b2", &commit1, false).unwrap();
        let b3 = repo.branch("b3", &commit2, false).unwrap();

        assert!(git_same_ref(repo, b1.get(), b2.get()));
        assert!(!git_same_ref(repo, b1.get(), b3.get()));
    }

    #[test]
    fn test_is_linear_ancestor() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit1 = create_commit(repo, "commit 1", &[]);
        let commit2 = create_commit(repo, "commit 2", &[&commit1]);
        let commit3 = create_commit(repo, "commit 3", &[&commit2]);

        // Ancestor check: commit1 is ancestor of commit3
        assert!(is_linear_ancestor(repo, commit1.id(), commit3.id()).unwrap());
        assert!(is_linear_ancestor(repo, commit2.id(), commit3.id()).unwrap());
        assert!(is_linear_ancestor(repo, commit1.id(), commit1.id()).unwrap());

        // Reverse is false
        assert!(!is_linear_ancestor(repo, commit3.id(), commit1.id()).unwrap());

        // Merge commit break linearity check
        let branch_commit = create_commit(repo, "branch commit", &[&commit1]);
        let merge_commit = create_commit(repo, "merge commit", &[&commit3, &branch_commit]);

        // Linear ancestor through merge commit should return false
        assert!(!is_linear_ancestor(repo, commit1.id(), merge_commit.id()).unwrap());
    }

    #[test]
    fn test_repository_clean() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        assert!(repository_clean(repo));

        // Create an untracked file, repository_clean should still be true if options ignore untracked
        let file_path = test_repo.path.join("untracked.txt");
        fs::write(&file_path, "hello").unwrap();
        assert!(repository_clean(repo));
    }

    #[test]
    fn test_to_branch() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit = create_commit(repo, "commit 1", &[]);
        let branch = repo.branch("feature-x", &commit, false).unwrap();
        let reference = branch.get();

        let found_branch = to_branch(repo, reference);
        assert_eq!(found_branch.name().unwrap().unwrap(), "feature-x");
    }
}


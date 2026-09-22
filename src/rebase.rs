#![deny(elided_lifetimes_in_paths)]

// rebase segment.
use git2::{
    Branch, BranchType, Error, Commit,
    Oid,
    Reference,
    Repository,RepositoryState,
    StatusOptions, StatusShow,

    CherrypickOptions,
    build::CheckoutBuilder,
};

use std::collections::HashMap;
use std::fs::{self,OpenOptions};
use std::io::{Write,self};
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{span, Level, debug, info, warn,error};
use colored::Colorize;

use thiserror;


use crate::utils::{iterator_symmetric_difference_indirect};

#[allow(unused)]
use crate::git_hierarchy::{GitHierarchy, Segment, Sum, load};
use crate::graph::discover::NodeExpander;

use crate::execute::git_run;
use crate::base::{checkout_new_head_at,
    staged_files,
    is_linear_ancestor,
    find_commit_in_reflog
};


pub enum RebaseResult {
    Nothing,
    Done,
    // Failed,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum RebaseError {
    #[error("hierarchy broken at {}", .0)]
    WrongHierarchy(String),
    #[error("repository in wrong state during rebase: {0:?}")]
    WrongState(git2::RepositoryState),
    #[error("marker file wrong: {0:?}")]
    WrongMarkerFile(String),
    #[error(transparent)]
    Git2(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Execute(#[from] crate::execute::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("rebase error")]
    Default,
}

const TEMP_HEAD_NAME: &str = "tempSegment";
const MARKER_FILENAME: &str = ".segment-cherry-pick";

fn marker_filename(repository: &Repository) -> PathBuf {
    repository.commondir().join(MARKER_FILENAME)
}

// see `cleanup_segment_rebase' which removes it.
fn create_marker_file(repository: &Repository, content: &str) -> io::Result<()> {
    let path = marker_filename(repository);
    // todo: use a Git reference instead?
    // persistent mark, if we fail, and during the session.
    debug!("Create marker: {:?}", path);
    fs::write(path, content)
}

/// Store persistently (between runs) the commit we stubled on
/// see `segment_to_continue'() for the read part
fn record_processed_commit(repository: &'_ Repository, oid: Oid, applied: bool) -> io::Result<()>{
    let path = marker_filename(repository);
    debug!("Update persistent state: {:?}", path);

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    let marker =
        if applied {
            "1"
        } else {
            "0"
        };
    writeln!(file, "{}", marker)?;
    writeln!(file, "{}", oid)?;
    debug!("{} {}", oid, marker);
    Ok(())
}


fn read_cherry_pick_head(repository: &'_ Repository) -> Result<String, io::Error> {
    fs::read_to_string(repository.commondir().join("CHERRY_PICK_HEAD"))
}

fn is_cherry_pick_applied(repository: &Repository) -> bool {
    if repository.state() == RepositoryState::CherryPick {
        return true;
    }
    if let Ok(index) = repository.index() {
        if index.has_conflicts() {
            return true;
        }
    }
    if let Ok(staged) = staged_files(repository) {
        if !staged.is_empty() {
            return true;
        }
    }
    false
}

/// Creates each commit during the rebase/cherry-picking: both in OK flow
/// and after manual intervention.  Can the user do the commit himself? -- do we setup the ....
/// @original is the original commit we try to clone.
///
fn commit_cherry_picked<'repo>(repository: &'repo Repository,
                               original: &Commit<'repo>,
                               parent_commit: &Commit<'repo>) -> Result<Oid, RebaseError> {
    let mut index = repository.index()?;
    if index.has_conflicts() {
        eprintln!("{}",Colorize::red("SORRY conflicts detected"));
        eprintln!("{}",Colorize::red("resolve them, and either commit or stage them"));

        // next time resume from this, `exclusive'.
        record_processed_commit(repository, original.id(), true)?;
        return Err(RebaseError::Default);
    }

    let statusses = staged_files(repository)?;
    if statusses.is_empty() {
        eprintln!("SORRY nothing staged, empty -- skip?");
        record_processed_commit(repository, original.id(), true)?;
        // so we have .git/CHERRY_PICK_HEAD ?
        return Err(RebaseError::Default);
    } else {
        info!("something staged");
    }

    let tree_oid = index.write_tree()?;
    let new_oid =
        if repository.head()?.peel_to_tree()?.id() == tree_oid {
            warn!("SORRY nothing staged, empty -- skip?");
            // bug: and no changes in the worktree!
            repository.head()?.target().ok_or_else(|| git2::Error::from_str("HEAD missing target"))?
            // silently skipping over?
            // exit(1);
        } else {
            // same tree id ... it was empty!

            //  "cannot create a tree from a not fully merged index."
            let tree = repository.find_tree(tree_oid)?;

            repository.commit(
                Some("HEAD"),
                // copy over:
                &original.author(),
                &original.committer(),
                original.message().ok_or_else(|| git2::Error::from_str("commit message missing"))?,
                // and timestamps? part of those ^^ !
                &tree,
                &[parent_commit],
            )?
        };

    repository.cleanup_state()?;
    Ok(new_oid)
}


// on top of HEAD
// cherry-picks each commits from the iterator, and returns the HEAD afterwards/on error?
fn cherry_pick_commits<'repo, T>(repository: &'repo Repository,
                                 iter: T,
                                 base_commit: Commit<'repo>)
                                 -> Result<Commit<'repo>, RebaseError>
    where T: Iterator<Item = Result<Oid, Error> >
{
    let mut current_commit = base_commit;
    for oid_res in iter {
        let oid = oid_res?;
        let to_apply = repository.find_commit(oid)?;

        info!("cherry-pick commit: {:?}", to_apply);

        let mut checkout_opts = CheckoutBuilder::new();
        checkout_opts.safe();
        let mut cherrypick_opts = CherrypickOptions::new();
        cherrypick_opts.checkout_builder(checkout_opts);

        let result = repository.cherrypick(&to_apply, Some(&mut cherrypick_opts));

        if let Err(e) = result {
            eprintln!("cherrypick failed on {}\n {:?}", to_apply.id(), e);
            eprintln!("error: code{:?}, class {:?}: {}", e.code(), e.class(), e.message());
            let applied = is_cherry_pick_applied(repository);
            record_processed_commit(repository, to_apply.id(), applied)?;

            let index = repository.index()?;
            if index.has_conflicts() {
                eprintln!("{}: SORRY conflicts detected", line!());
            }

            return Err(RebaseError::Git2(e));
        }

        let new_oid = commit_cherry_picked(repository, &to_apply, &current_commit)?;
        current_commit = repository.find_commit(new_oid)?;
    }

    Ok(current_commit)
}

/// Given a @segment, and HEAD ....
/// either exit or rewrite the segment ....its reference should update oid.
pub fn rebase_segment<'repo>(repository: &'repo Repository, segment: &Segment<'repo>) -> Result<RebaseResult, RebaseError> {
    if segment.uptodate(repository) {
        info!("nothing to do -- base and start equal");
        return Ok(RebaseResult::Nothing);
    }

    let new_start = segment.base(repository).peel_to_commit()?;

    if segment.empty(repository)? {
        return rebase_empty_segment(segment, repository);
    }

    // fixme: if we are in the middle of rebase?
    if repository.state() != RepositoryState::Clean {
        error!("the repository is not clean");
        return Err(RebaseError::WrongState(repository.state()));
    }

    info!("rebase_segment: {}", segment.name());
    debug!("rebasing by Cherry-picking {}!", segment.name());

    create_marker_file(repository, &format!("{}\n", segment.name()))?;

    checkout_new_head_at(repository, None, &new_start);

    let sha = new_start.id();
    debug!("set-head: {:?}", &sha);
    repository.set_head_detached(sha)?;
    if let Ok(head) = repository.head() {
        debug!("checkout: {:?}", head.name());
    }
    // bug: goes out of sync.
    if false {
        if !git_run(
            repository,
            &["cherry-pick", "--", segment.git_revisions().as_str()],
        )
            .is_ok_and(|x| x.success())
        {
            debug!("git cherry-pick failed");
            return Err(RebaseError::Default)
        } else {
            return Ok(RebaseResult::Done);
        }
    } else {
        let commit = cherry_pick_commits(repository,
                                         segment.iter(repository)?,
                                         segment.base(repository).peel_to_commit()?
                                         )?;
        // move
        segment.reset(repository, commit.id(), "rebased")?;
    }

    cleanup_segment_rebase(repository, segment);
    Ok(RebaseResult::Done)
}

// The old, using git(1)
#[allow(unused)]
fn rebase_continue_git1(repository: &Repository, segment_name: &str) -> Result<RebaseResult, RebaseError> {
    if !git_run(repository, &["cherry-pick", "--continue"]).is_ok_and(|x| x.success()) {
        info!("git cherry-pick --continue failed");
        return Err(RebaseError::Default);
    }

    if let GitHierarchy::Segment(segment) = load(repository, segment_name)? {
        let tmp_head: Branch<'_> = repository
            .find_branch(TEMP_HEAD_NAME, BranchType::Local)
            .unwrap();
        if tmp_head.is_head() {
            //name: &str, branch_type: BranchType) -> Result<Branch<'_>, Error> {head();
            panic!("rebase_segment_finish not supported anymore");
            cleanup_segment_rebase(repository, &segment);
            Ok(RebaseResult::Done)
        } else {
            // mismatch
            Err(RebaseError::Default)
        }
    } else {
        Ok(RebaseResult::Nothing)
    }
}

/// resume rebasing segment, from certain commit, exclusive/inclusive based on `skip'.
// HEAD is already correct.
// can the status be still CHERRY_PICK ?
fn continue_segment_cherry_pick<'repo>(repository: &'repo Repository,
                                       segment: &'_ Segment<'repo>,
                                       commit_id: Oid,
                                       skip: usize
) -> Result<(), RebaseError> {
    // Find & skip:
    let iter = segment.iter(repository)?
        .skip_while(|x| x.as_ref().map_or(false, |oid| oid != &commit_id));

    let mut peek = iter.peekable();
    if peek.peek().is_none() {
        debug!("Couldn't find the commmit on the segment");
        return Err(RebaseError::WrongHierarchy(segment.name().to_owned()));
    }

    // todo: check the index
    let parent = repository.head()?.peel_to_commit()?;

    // here we continue the whole sub-segment chain:
    debug!("now continue to pick the rest of the segment '{}'", segment.name());

    // check we are in a clean state!
    // The default, if unspecified, is to show the index and the working
    let statuses = repository.statuses(None)?;
    if ! statuses.len() == 0 {
        eprintln!("Status is not clean!");
        return Err(RebaseError::WrongState(repository.state()));
    }

    let commit = cherry_pick_commits(repository,
                                     peek.skip(skip),
                                     parent)?;
    // might need this if nothing to cherrypick anymore.
    segment.reset(repository, commit.id(), "rebased")?;
    Ok(())
}


/// Loads the persistent state: the commit we last processed.
///
/// Returns `Ok(Some((segment_name, Some((commit_id, skip)))))` where:
/// - `segment_name`: the name of the segment being rebased.
/// - `commit_id`: the OID of the commit last processed.
/// - `skip`: count indicating whether to skip or resume from `commit_id`
///   (for instance if merge conflicts occurred or applying the next commit failed).
/// Returns `Ok(None)` if no rebase marker file exists.
pub fn segment_to_continue(repository: &Repository) -> Result<Option<(String,Option<(String,usize)>)>, RebaseError>
{
    let path = marker_filename(repository);

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let mut lines = content.lines();

    let segment_name = lines.next().ok_or(RebaseError::WrongMarkerFile("marker file empty".to_string()))?.trim().to_owned();
    if segment_name.is_empty() ||
        !Segment::name_is_valid(&segment_name).map_err(|_| RebaseError::WrongMarkerFile("invalid segment name in the marker file".to_string()))? {
        return Err(RebaseError::WrongMarkerFile("empty segment name in the marker file".to_string()));
    }

    // this can fail: if we failed on the last commit, at the moment of commit -- empty or whatever.
    match lines.next_back() {
        None =>
            Ok(Some((segment_name, None))),
        Some(oid) => {
            let skip_str = lines.next_back().ok_or(RebaseError::WrongMarkerFile("no skip in the marker file".to_string()))?;
            let skip: usize = skip_str.parse().map_err(|_| RebaseError::WrongMarkerFile("non-numeric skip in the marker file".to_string()))?;
            debug!("from file: continue on {}, after {:?}", segment_name, oid);
            Ok(Some((segment_name, Some((oid.to_owned(), skip)))))
        }
    }
}

// Continue after an issue:
// either cherry-pick conflicts resolved by the user, or
// he left mess, and ....on detached head. Unlike other tools.
pub fn rebase_segment_continue(repository: &Repository) -> Result<RebaseResult, RebaseError> {
    // todo: this might be the input:
    let (segment_name, rest) = segment_to_continue(repository)?.ok_or(RebaseError::Default)?;
    let skip;

    if let GitHierarchy::Segment(segment) = load(repository, &segment_name)? {
        let commit_id =
            if repository.state() == RepositoryState::CherryPick {
                // read the CHERRY_PICK_HEAD
                // todo: convert to step.step2...
                // mmc: so this is the same as `oid' ?
                let cherry_pick_str = read_cherry_pick_head(repository).map_err(|_| RebaseError::Default)?;
                let commit_id = Oid::from_str(cherry_pick_str.trim()).map_err(|_| RebaseError::Default)?;
                debug!("should continue the cherry-pick {:?}", commit_id);

                let mut option =  StatusOptions::new();
                option.show(StatusShow::Index);
                let statuses = repository.statuses(Some(&mut option))?;
                if ! statuses.is_empty() {
                    debug!("so we have {} changed files", statuses.len());
                    // if !repository.index().unwrap().is_empty()
                    // fixme: this is misleading!

                    // commit it, or reset the state?
                    debug!("non-empty index -> commit...");
                    let to_apply = repository.find_commit(commit_id)?;

                    let parent = repository.head()?.peel_to_commit()?;
                    let new_oid = commit_cherry_picked(repository,
                                                       // todo: it's okay to skip:
                                                       &to_apply,
                                                       &parent)?;
                    debug!("new commit created {new_oid}");
                } else {
                    // the user might have decided to drop this change -- skip over.
                    info!("Cleaning cherry pick info: user unstaged the change");
                    repository.cleanup_state()?;
                }
                skip = 1;
                // we need the next one.
                commit_id
            } else {
                debug!("so cherry-pick finished, for some reason we need to continue");
                if let Some((oid, stored_skip)) = rest {
                    skip = stored_skip;
                    Oid::from_str(&oid).map_err(|_| RebaseError::WrongMarkerFile("invalid OID in marker file".to_string()))?
                } else {
                    return Err(RebaseError::WrongMarkerFile("no commit info to continue from".to_string()));
                }
            };

        eprintln!("should cherry-pick starting from oid {} + {}", commit_id, skip);
        // so we should save it now!
        record_processed_commit(repository, commit_id, skip != 0)?;

        continue_segment_cherry_pick(repository, &segment, commit_id, skip)?;

        let head_commit = repository.head()?.peel_to_commit()?;
        segment.reset(repository, head_commit.id(), "rebased")?;

        cleanup_segment_rebase(repository, &segment);
        Ok(RebaseResult::Done)
    } else {
        Err(RebaseError::WrongHierarchy(segment_name))
    }
}

// bad name:
fn cleanup_segment_rebase(repository: &Repository, _segment: &Segment<'_>) {
    let path = marker_filename(repository);
    debug!("delete marker: {:?}", path);
    // todo: raise the error!
    let _ = fs::remove_file(path);
}

fn rebase_empty_segment<'repo>(
    segment: &Segment<'repo>,
    repository: &'repo Repository,
) -> Result<RebaseResult, RebaseError> {
    debug!("rebase empty segment: {}", segment.name());

    segment.reset(repository,
                  segment.base(repository).peel_to_commit()?.id(),
                  "rebased")?;
    Ok(RebaseResult::Done)
}

pub fn check_segment(repository: &Repository, segment: &Segment<'_>) -> Result<(), RebaseError>
{
    // no merge commits
    if ! is_linear_ancestor(repository,
                            segment.start(),
                            segment.reference.borrow().target().unwrap())? {
        warn!("check_segment failed for {}", segment.name());
        return Err(RebaseError::WrongHierarchy(segment.name().to_owned()));
    }

    // no segments inside. lenght limited....

    // git_revisions()
    // walk.push_ref(segment.reference.borrow());
    // walk.hide(segment.start.target().unwrap());
    // walk.hide_ref(ref);

    // push_range
    // descendant of start.

    // start.is_ancestor(reference);
    Ok(())
}

/// more heuristics, more permissive
fn ref_related_to(repo: &Repository,
    branch: &Reference<'_>,
    commit: Oid) -> bool {      // Commit<'_>
    // if both:
    // take 1 1
    // if ancestor, or in reflog.

    // ancestor
    if is_linear_ancestor(repo, commit, branch.target().unwrap()).unwrap() {
        true
    } else {
        // reflog
        if find_commit_in_reflog(repo, branch.name().expect("should have a name"), commit).is_ok_and(|x| x.is_some()){
            return true;
        }
        false
    }
}

fn match_commits_to_references<'repo>( //  A,B
    repo: &Repository,
    a_vec: Vec<Reference<'repo>>,
    mut b_vec: Vec<Oid>,
) -> (Vec<Reference<'repo>>, Vec<Oid>) {
    // Vec<(&'a Reference, &'a Oid)>,

    let mut pairs = Vec::new();
    let mut unmatched_a = Vec::new();

    for a in a_vec {
        match b_vec.iter().position(|b| ref_related_to(repo, &a, *b)) {
            Some(i) => {
                debug!("found history relation! {}", a.name().unwrap());
                let b = b_vec.swap_remove(i);
                pairs.push((a, b));
            }
            None => unmatched_a.push(a),
        }
    }

    // pairs
    if ! unmatched_a.is_empty() {
        warn!("Not matched");
        for r in &unmatched_a {
            warn!("{}", r.name().unwrap());
        }
    }

    if ! b_vec.is_empty() {
        warn!("Still some commits not matched");
    }

    (unmatched_a, b_vec)

}


pub fn match_summands_to_parents<'repo, 'a>(
    _repository: &'repo Repository,
    parent_commits: &[Oid],
    summands: &'a Vec<&'a GitHierarchy<'repo>>) -> (Vec<Oid>, Vec<&'a &'a GitHierarchy<'repo>>) {

        // return ( /* mapping*/ loose_parents, moved_components, )
    iterator_symmetric_difference_indirect(
        parent_commits.iter().copied(),
        summands, // & fails
        // mapping
        |gh| {
            debug!("mapping {:?} to {:?}", gh.node_identity(),
                gh.commit().unwrap().id());
            gh.commit().unwrap().id()
        }
    )
}

pub fn check_summands<'repo>(
    repository: &'repo Repository,
    sum: &Sum<'repo>,
    parent_commits: &[Oid],
    summands: &Vec<&GitHierarchy<'repo>>)
 -> Result<(), RebaseError>
{
    // I need a mapping function
    // iter1, iter2, map-domain2-to-domain1
    let (unknown_parents, summands_away) = match_summands_to_parents(repository, parent_commits, summands);

    // now map permissively:
    // if the summand moved up.
    // parent ... find whose ancestor it is.


    if !(summands_away.is_empty() && unknown_parents.is_empty()) {
        warn!("sum {} is not up-to-date. Looking closer...", sum.name());

        // one more attempt
        // convert gh -> reference
        let summands_references = summands_away.iter().map(|x| x.reference_clone(repository).expect("gh should have reference associated") ).collect();

        let (summands_refs_away, unknown_parents) = match_commits_to_references(repository, summands_references, unknown_parents);

        if ! summands_refs_away.is_empty() {
            warn!("some summands are not in parents:");
            for i in summands_refs_away {
                warn!("{}", i.name().unwrap());
            }
        } else {
            if unknown_parents.is_empty() {
                info!("solved all, summands only grew");
                return Ok(());
            }
        }

        if !unknown_parents.is_empty() {
            warn!("some parents are not in summands");
            for i in unknown_parents {
                warn!("{}", i);
            }
        }

        return Err(RebaseError::WrongHierarchy(sum.name().to_owned()));
    }
    /*
    (mapped, rest_summands, left_overs_parent_commits) = distribute(sum);
    // either it went ahead ....or? what if it's rebased?m
       
    for bad in rest_summands {
        // try to find in over
        find_ancestor()
    }

    */

    Ok(())
}

pub fn check_sum<'repo>(
    repository: &'repo Repository,
    sum: &Sum<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
) -> Result<(), RebaseError> {

    // terrible:
    // !i>2 in Rust  means ~i>2 in C
    // https://users.rust-lang.org/t/why-does-rust-use-the-same-symbol-for-bitwise-not-or-inverse-and-logical-negation/117337/2
    let count = sum.summand_count();
    if count <= 1 {
        warn!("not a merge: {}, only {} parent commits", sum.name(), count);
        return Err(RebaseError::WrongHierarchy(sum.name().to_owned()));
    }

    // each of the summands has relationship to a parent commit.
    let summands = sum.summands(repository);
    /* assumption:
    sum has its summands   base/1 ... base/N
    these might resolve to References. -- how is that different from Branch?

    During the rebasing we change ... Branches (References), and update them in the `object_map'
    so we .... prefer to look up there.
     */

    // find the representation which we already have and keep updating.

    // Map through object_map to the Nodes:
    let graphed_summands: Vec<&GitHierarchy<'_>> = summands
        .iter()
        .map(
            |s| {
                let gh = object_map.get(s.name().unwrap()).unwrap();
                debug!(
                    "convert {:?} to {:?}",
                    s.name().unwrap(),
                    gh.node_identity()
                );
                gh
            })
        .collect();

    let parent_commits = sum.parent_commits();

    debug!("The current parent commits are: {:?}", parent_commits);
    for c in sum.parent_commits() {
        debug!("  {}", c);
    }

    check_summands(repository, sum, &parent_commits, &graphed_summands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestRepo;
    use std::fs;

    #[test]
    fn test_rebase_error_display_and_conversions() {
        let err1 = RebaseError::WrongHierarchy("branch-a".to_string());
        assert_eq!(err1.to_string(), "hierarchy broken at branch-a");

        let err2 = RebaseError::WrongState(git2::RepositoryState::Rebase);
        assert_eq!(err2.to_string(), "repository in wrong state during rebase: Rebase");

        let err2 = RebaseError::WrongMarkerFile("bad file".to_string());
        assert_eq!(err2.to_string(), "marker file wrong: \"bad file\"".to_string());

        let err3 = RebaseError::Default;
        assert_eq!(err3.to_string(), "rebase error");

        let git_err = git2::Error::from_str("some git error");
        let converted_git: RebaseError = git_err.into();
        assert!(matches!(converted_git, RebaseError::Git2(_)));
        assert_eq!(converted_git.to_string(), "some git error");

        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "io error");
        let converted_io: RebaseError = io_err.into();
        assert!(matches!(converted_io, RebaseError::Io(_)));
        assert_eq!(converted_io.to_string(), "io error");

        let exec_err = crate::execute::Error::NoWorkDir;
        let converted_exec: RebaseError = exec_err.into();
        assert!(matches!(converted_exec, RebaseError::Execute(_)));
        assert_eq!(converted_exec.to_string(), "Repository has no working directory");

        let anyhow_err = anyhow::anyhow!("custom anyhow error");
        let converted_anyhow: RebaseError = anyhow_err.into();
        assert!(matches!(converted_anyhow, RebaseError::Anyhow(_)));
        assert_eq!(converted_anyhow.to_string(), "custom anyhow error");
    }

    #[test]
    fn test_marker_file_creation() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let path = marker_filename(repo);
        assert!(path.ends_with(MARKER_FILENAME));

        create_marker_file(repo, "test-content").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "test-content");
    }

    #[test]
    fn test_record_processed_commit() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit = crate::test_utils::create_commit(repo, "test commit", &[]);
        create_marker_file(repo, "segment_name\n").unwrap();

        record_processed_commit(repo, commit.id(), true).unwrap();

        let path = marker_filename(repo);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(&commit.id().to_string()));
        assert!(content.contains("1"));
    }

    #[test]
    fn test_segment_to_continue_corrupt_marker() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        assert!(segment_to_continue(repo).unwrap().is_none());

        create_marker_file(repo, "").unwrap();
        assert!(segment_to_continue(repo).is_err());

        create_marker_file(repo, "feature\nnot_a_number\nsome_oid\n").unwrap();
        assert!(segment_to_continue(repo).is_err());
    }
}



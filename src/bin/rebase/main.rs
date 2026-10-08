#![deny(elided_lifetimes_in_paths)]
// walk the hierarchy
// - assemble list of segments/sums.
// - graph, toposort

use clap::Parser;
#[allow(unused_imports)]
use git2::{Branch, BranchType, Error, Commit, Reference, ReferenceFormat, Repository,
           MergeOptions,
           build::CheckoutBuilder,
           Oid,
           RepositoryState,
           // merge:
           AnnotatedCommit,
};

#[allow(unused_imports)]
use tracing::{span, Level, debug, info, warn,error};

use ::git_hierarchy::base::{checkout_new_head_at, extract_remote_name, git_same_ref, upstream_of, to_branch};
use ::git_hierarchy::cli::{ClapGitRepo, resolve_reference_names_from_user};
use ::git_hierarchy::execute::git_run;
use ::git_hierarchy::utils::{iterator_symmetric_difference, init_tracing,
};
use ::git_hierarchy::rebase::{check_segment, check_sum,
                              rebase_segment,rebase_segment_continue,
                              segment_to_continue,
                              RebaseResult, RebaseError};
use std::collections::HashMap;
use std::iter::Iterator;

use crate::graph::discover_pet::find_hierarchy;

// I need both:
#[allow(unused)]
use ::git_hierarchy::git_hierarchy::{GitHierarchy, Segment, Sum, load};

use anyhow::{Context, Result, anyhow, bail};
use colored::Colorize;

/*
 note: ambiguous because of a conflict between a name from a glob import and an outer scope during import or macro resolution
   = note: `git_hierarchy` could refer to a crate passed with `--extern`
   = help: use `::git_hierarchy` to refer to this crate unambiguously
*/

use ::git_hierarchy::graph;
use graph::discover::NodeExpander;


/// Compose commit message for the Sum/Merge of .... components given by the
/// first/others.
fn get_merge_commit_message<'a, 'b, 'c, Iter>(
    sum_name: &'b str,
    first: &'c str,
    others: Iter,
) -> String
where
    Iter: Iterator<Item = &'a str>,
{
    let mut message = format!("Sum: {sum_name}\n\n{}", first);

    const NAMES_PER_LINE: usize = 3;
    for (i, name) in others.enumerate() {
        message.push_str(" + ");
        message.push_str(name);

        if i % NAMES_PER_LINE == 0 {
            // exactly same as push_str()
            message += "\n"
        }
    }
    message
}

/// Given @sum, check if it's up-to-date.
///
/// If not: create a new git merge commit.
fn remerge_sum<'repo>(
    repository: &'repo Repository,
    sum: &Sum<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>, // this lifetime
) -> Result<RebaseResult, RebaseError> {
    let summands = sum.summands(repository);

    /* assumption:
    sum has its summands   base/1 ... base/N
    these might resolve to References. -- how is that different from Branch?

    During the rebasing we change ... Branches (References), and update them in the `object_map'
    so we .... prefer to look up there.
     */

    // find the representation which we already have and keep updating.
    let graphed_summands: Vec<&GitHierarchy<'_>> = summands
        .iter()
        .map(
            |s| {
                let gh = object_map.get(s.name().unwrap()).unwrap();
                debug!(
                    "resolve {:?} to {:?}",
                    s.name().unwrap(),
                    gh.node_identity()
                );
                gh
            })
        .collect();

    for s in &graphed_summands {
        if !Segment::name_is_valid(s.node_identity())? {
            return Err(RebaseError::WrongHierarchy(format!(
                "invalid summand name: {}",
                s.node_identity()
            )));
        }
    }

    let parent_commits = sum.parent_commits();

    debug!("The current parent commits are: {:?}", parent_commits);
    for c in sum.parent_commits() {
        debug!("  {}", c);
    }

    let (orhan_summands, extra_parents) = iterator_symmetric_difference(
        graphed_summands.iter().map(|gh| {
            debug!("{:?} is commit {:?}", gh.node_identity(),
                   gh.commit().unwrap().id());
            gh.commit().unwrap().id()
        }),
        parent_commits);


    if orhan_summands.is_empty() && extra_parents.is_empty() {
        info!("sum is up2date: summands & parent commits align");
    } else {
        info!("so the sum is not up-to-date!");

        let first = graphed_summands.first().unwrap();

        let message = get_merge_commit_message(
            sum.name(),
            first.node_identity(), // : &GitHierarchy
            graphed_summands.iter()
                .skip(1).map(|x| x.node_identity()),
        );

        if graphed_summands.len() > 2 {
            checkout_new_head_at(repository, None, &first.commit()?);

            // use  git_run or?
            let mut cmdline = vec![
                "merge",
                "-m",
                &message, // why is this not automatic?
                "--rerere-autoupdate",
                "--strategy",
                "octopus",
                "--strategy",
                "recursive",
                "--strategy-option",
                "patience",
                "--strategy-option",
                "ignore-space-change",
                "--",
            ];
            cmdline.extend(graphed_summands.iter().map(|s| s.node_identity()));

            let status = git_run(repository, &cmdline)?;
            // status.exit_ok().or_else(|e| Err(RebaseError::Default))?;
            if status.code() != Some(0) {
                return Err(RebaseError::Default);
            }
            // "commit": move the SUM head with reflog message:
            sum.reset(repository.head()?.resolve()?.target().unwrap());
        } else {
            // libgit2
            assert!(checkout_new_head_at(repository, None, &first.commit()?).is_none());

            // Options:
            let mut merge_opts = MergeOptions::new();
            merge_opts.fail_on_conflict(true)
                .standard_style(true)
                .ignore_whitespace(true)
                .patience(true)
                .minimal(true)
                ;

            let mut checkout_opts = CheckoutBuilder::new();
            checkout_opts.safe();


            // one more conversion:
            // we have Vec<GitHierarchy> ->  Vec<Commit> need..... Vec<AnnotatedCommit>
            //
            let annotated_commits : Vec<AnnotatedCommit<'_>> =
                graphed_summands.iter().skip(1).map(
                    |gh| {
                        // fixme:
                        let oid = gh.commit().unwrap().id();
                        repository.find_annotated_commit(oid).unwrap()
                    }).collect();
            // the references vec:
            let annotated_commits_refs = annotated_commits.iter().collect::<Vec<_>>();

            debug!("Calling merge()");
            repository.merge(
                &annotated_commits_refs,
                Some(&mut merge_opts),
                Some(&mut checkout_opts)
            ).expect("Merge should succeed");

            // make if a function:
            // oid = save_index_with( message, signature);
            let mut index = repository.index().map_err(RebaseError::Git2)?;
            if index.has_conflicts() {
                info!("{}: SORRY conflicts detected", line!());
                return Err(RebaseError::Default);
            }
            let id = index.write_tree().unwrap();
            let tree = repository.find_tree(id).unwrap();

            let sig = repository.signature().unwrap();


            // Create the commit:
            // another one: Reference -> Commit -> Oid >>> lookup >>> AnnotatedCommit->Oid
            let commits : Vec<Commit<'_>> = graphed_summands.iter()
                .map(|gh| gh.commit().unwrap()) // fixme!
                .collect();
            // references
            let commits_refs = commits.iter().collect::<Vec<_>>();

            debug!("Calling merge()");

            let new_oid = repository.commit(
                Some("HEAD"),
                &sig, // author(),
                &sig, // committer(),
                &message,
                &tree,
                // this is however already stored in the directory:
                &commits_refs,
                // Error: "failed to create commit: current tip is not the first parent"
            ).unwrap();

            repository.cleanup_state().expect("cleaning up should succeed");

            // this both on the Repo/storer both here in our Data ?
            sum.reset(new_oid);
        }
    }

    // do we have a hint -- another merge?
    // git merge
    Ok(RebaseResult::Done)
}

fn fetch_upstream_of(repository: &Repository, reference: &Reference<'_>) -> Result<(), Error> {
    // resolve what to fetch.
    if reference.is_remote() {
        let name = reference.name().ok_or_else(|| Error::from_str("reference missing name"))?;
        let (remote_name, branch) = extract_remote_name(name)
            .ok_or_else(|| Error::from_str("invalid remote reference format"))?;
        let mut remote = repository.find_remote(remote_name)?;
        debug!("fetching from remote {:?}: {:?}", remote_name, branch);

        // FetchOptions, message
        if remote.fetch(&[branch], None, Some("part of poset-rebasing")).is_err() {
            return Err(Error::from_str("Fetch failed"));
        }
    } else if reference.is_branch() { // and we know it's not Segment/Sum, right?
        // the user has a reason to use local branch.
        // So we don't want to change it (by fetching) without explicit permission.
        // implicit permission -- that it's just following a remote branch.
        let name = Reference::normalize_name(reference.name().unwrap(), ReferenceFormat::NORMAL).unwrap();

        info!("fetch local {name}");
        // why redo this? see above ^^

        let mut branch = to_branch(repository, reference);
        if let Some((mut remote, remote_branch, remote_branch_name)) = upstream_of(repository, &branch) {

            if git_same_ref(repository, reference, remote_branch.get())? {
                // we might be behind?
                debug!("in sync, so let's fetch & update");
            } else {
                // Check if still in sync, to not lose local changes.
                return Err(Error::from_str(&format!(
                    "{} not in sync with upstream {}; should not update.",
                    name,
                    remote_branch.name().ok().flatten().unwrap_or("")
                )));
            }

            info!("fetch {} {} ....", remote.name().unwrap(), remote_branch_name);
            if remote.fetch(&[remote_branch_name], None, None).is_ok() {
                let oid = branch
                    .upstream()
                    .unwrap()
                    .get()
                    .target()
                    .expect("upstream disappeared");
                // wtf?
                branch
                    .get_mut()
                    .set_target(oid, "fetch & fast-forward")
                    .expect("fetch/sync failed");
            }
        }
    } else {
        // fixme!
    }
    Ok(())
}

fn rebase_node<'repo>(
    repo: &'repo Repository,
    node: &GitHierarchy<'repo>,
    fetch: bool,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
) -> Result<RebaseResult> {
    match node {
        GitHierarchy::Name(n) => {
            bail!("invalid Name node variant in rebase_node: {}", n);
        }
        GitHierarchy::Reference(r) => {
            if fetch {
                fetch_upstream_of(repo, r)
                    .with_context(|| format!("failed to fetch upstream of '{}'", r.name().unwrap_or("")))?;
            }
            Ok(RebaseResult::Done)
        }
        GitHierarchy::Segment(segment) => {
            let my_span = span!(Level::INFO, "segment", name = segment.name());
            let _enter = my_span.enter();
            rebase_segment(repo, segment)
                .with_context(|| format!("failed to rebase segment '{}'", segment.name()))
        }
        GitHierarchy::Sum(sum) => {
            let _my_span = span!(Level::INFO, "sum", name = sum.name());
            remerge_sum(repo, sum, object_map)
                .with_context(|| format!("failed to remerge sum '{}'", sum.name()))
        }
    }
}

fn check_node<'repo>(
    repo: &'repo Repository,
    node: &GitHierarchy<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
) -> Result<()> {
    match node {
        GitHierarchy::Name(n) => {
            bail!("invalid Name node variant in check_node: {}", n);
        }
        GitHierarchy::Reference(_r) => {
            // no
        }
        GitHierarchy::Segment(segment) => {
            check_segment(repo, segment)
                .with_context(|| format!("check failed for segment '{}'", segment.name()))?;
        }
        GitHierarchy::Sum(sum) => {
            check_sum(repo, sum, object_map)
                .with_context(|| format!("check failed for sum '{}'", sum.name()))?;
        }
    }

    Ok(())
}


// whole hierarchy
fn rebase_tree(
    repository: &Repository,
    root: String,
    fetch: bool,
    ignore: &[String],
    skip: &[String],
) -> Result<()> {
    debug!("find the hierarchy from {}", &root);
    tracing::debug_span!("hierarchy");
    let hierarchy_graph = find_hierarchy(repository, root);

    // verify we can do it:
    debug!("Verify");
    for v in &hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph
            .labeled_objects
            .get(v)
            .ok_or_else(|| anyhow!("vertex '{}' missing from hierarchy objects", v))?;
        let name = vertex.node_identity();
        debug!(
            "{:?} -> ({:?} / {:?})",
            v,
            name,
            hierarchy_graph.graph
                .node_weight(*hierarchy_graph.labeled_nodes.get(v).unwrap())
                .unwrap()
        );
        if ignore.iter().any(|x| x == v) {
            info!("not checking: {name}");
            continue;
        }
        check_node(repository, vertex, &hierarchy_graph.labeled_objects)?;
    }

    debug!("Rebasing");
    for v in &hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph
            .labeled_objects
            .get(v)
            .ok_or_else(|| anyhow!("vertex '{}' missing from hierarchy objects", v))?;
        let name = vertex.node_identity();

        if skip.iter().any(|x| x == name) {
            info!("Skipping: {name}");
            continue;
        }

        debug!(
            "rebase node {:?} => {:?} {:?}",
            v,
            name,
            hierarchy_graph.graph
                .node_weight(*hierarchy_graph.labeled_nodes.get(v).unwrap())
                .unwrap()
        );
        rebase_node(repository, vertex, fetch, &hierarchy_graph.labeled_objects)?;
    }
    debug!("done");
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[arg(short='f', long="no-fetch" )]
    no_fetch: bool,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long = "continue")]
    cont: bool,
    root_reference: Option<String>,

    #[arg(short, long = "ignore")]
    ignore: Vec<String>,

    #[arg(short, long = "skip")]
    skip: Vec<String>,

    #[arg(short = 'n', long = "dry", aliases = ["dry-run"])]
    dry: bool,

    #[arg(long = "detect-conflicts", aliases = ["conflicts"])]
    detect_conflicts: bool,
}

fn rebase_tree_dry(
    repository: &Repository,
    root: String,
    ignore: &[String],
    skip: &[String],
    detect_conflicts: bool,
) -> Result<()> {
    debug!("find the hierarchy from {}", &root);
    let hierarchy_graph = find_hierarchy(repository, root);

    // Verify
    for v in &hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph
            .labeled_objects
            .get(v)
            .ok_or_else(|| anyhow!("vertex '{}' missing from hierarchy objects", v))?;
        if ignore.iter().any(|x| x == v) {
            continue;
        }
        check_node(repository, vertex, &hierarchy_graph.labeled_objects)?;
    }

    println!("Dry run steps:");
    let mut sim_tips: HashMap<String, Commit<'_>> = HashMap::new();
    let mut conflicts_found = false;

    for v in &hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph
            .labeled_objects
            .get(v)
            .ok_or_else(|| anyhow!("vertex '{}' missing from hierarchy objects", v))?;
        let name = vertex.node_identity();

        if skip.iter().any(|x| x == name) {
            println!("  [dry-run] Skip: {}", name);
            continue;
        }

        match vertex {
            GitHierarchy::Name(n) => {
                bail!("invalid Name node variant in rebase_tree_dry: {}", n);
            }
            GitHierarchy::Reference(r) => {
                let ref_name = r.name().unwrap_or(name);
                println!("  [dry-run] Reference '{}' (no fetch)", ref_name);
                if let Ok(c) = r.peel_to_commit() {
                    sim_tips.insert(name.to_owned(), c);
                }
            }
            GitHierarchy::Segment(segment) => {
                let base_ref_symbolic = segment.base(repository);
                let base_ref_name = base_ref_symbolic.name().unwrap_or("");

                let base_commit = if let Some(c) = sim_tips.get(base_ref_name) {
                    c.clone()
                } else {
                    base_ref_symbolic.peel_to_commit()?
                };

                let base_changed = sim_tips.contains_key(base_ref_name);

                if !base_changed && segment.uptodate(repository) {
                    println!("  [dry-run] Segment '{}' is up to date", segment.name());
                    if let Ok(c) = segment.reference.borrow().peel_to_commit() {
                        sim_tips.insert(name.to_owned(), c);
                    }
                } else if segment.empty(repository)? {
                    println!(
                        "  [dry-run] Rebase empty segment '{}' onto '{}'",
                        segment.name(),
                        base_ref_name
                    );
                    sim_tips.insert(name.to_owned(), base_commit);
                } else {
                    let commits_res: Result<Vec<Oid>, Error> =
                        segment.iter(repository)?.collect();
                    let commits = commits_res?;
                    println!(
                        "  [dry-run] Rebase segment '{}' onto '{}' ({} commit{})",
                        segment.name(),
                        base_ref_name,
                        commits.len(),
                        if commits.len() == 1 { "" } else { "s" }
                    );

                    let mut current_commit = base_commit;
                    if detect_conflicts {
                        let empty_tree = repository.treebuilder(None).and_then(|b| b.write()).and_then(|id| repository.find_tree(id)).ok();
                        for oid in &commits {
                            let commit_to_apply = repository.find_commit(*oid)?;
                            let ancestor_tree = if let Ok(parent_commit) = commit_to_apply.parent(0) {
                                parent_commit.tree().ok()
                            } else {
                                empty_tree.clone()
                            };
                            let ancestor_tree = ancestor_tree.as_ref().or(empty_tree.as_ref()).unwrap();
                            let our_tree = current_commit.tree()?;
                            let their_tree = commit_to_apply.tree()?;

                            let mut merge_opts = MergeOptions::new();
                            merge_opts.patience(true).ignore_whitespace(true);

                            let mut index = repository.merge_trees(
                                ancestor_tree,
                                &our_tree,
                                &their_tree,
                                Some(&mut merge_opts),
                            )?;

                            if index.has_conflicts() {
                                let summary = commit_to_apply.summary().unwrap_or("");
                                println!(
                                    "    [conflict] Segment '{}': conflict on commit {} (\"{}\")",
                                    segment.name(),
                                    &oid.to_string()[..7],
                                    summary
                                );
                                conflicts_found = true;
                                break;
                            } else {
                                let tree_oid = index.write_tree_to(repository)?;
                                let tree = repository.find_tree(tree_oid)?;
                                let sig = repository.signature().unwrap_or_else(|_| {
                                    git2::Signature::now("DryRun", "dry@run").unwrap()
                                });
                                let new_oid = repository.commit(
                                    None,
                                    &sig,
                                    &sig,
                                    commit_to_apply.message().unwrap_or(""),
                                    &tree,
                                    &[&current_commit],
                                )?;
                                current_commit = repository.find_commit(new_oid)?;
                            }
                        }
                    }
                    sim_tips.insert(name.to_owned(), current_commit);
                }
            }
            GitHierarchy::Sum(sum) => {
                let summands = sum.summands(repository);
                let mut summand_commits: Vec<Commit<'_>> = Vec::new();
                let mut summand_names: Vec<String> = Vec::new();

                for s in &summands {
                    let s_name = s.name().unwrap_or("");
                    summand_names.push(s_name.to_owned());
                    if let Some(c) = sim_tips.get(s_name) {
                        summand_commits.push(c.clone());
                    } else {
                        let gh = hierarchy_graph
                            .labeled_objects
                            .get(s_name)
                            .ok_or_else(|| anyhow!("summand '{}' not found", s_name))?;
                        summand_commits.push(gh.commit()?);
                    }
                }

                let summands_changed = summand_names
                    .iter()
                    .any(|n| sim_tips.contains_key(n));

                let parent_commits = sum.parent_commits();
                let (orhan_summands, extra_parents) = iterator_symmetric_difference(
                    summand_commits.iter().map(|c| c.id()),
                    parent_commits,
                );

                if !summands_changed && orhan_summands.is_empty() && extra_parents.is_empty() {
                    println!("  [dry-run] Sum '{}' is up to date", sum.name());
                    if let Ok(c) = sum.reference.borrow().peel_to_commit() {
                        sim_tips.insert(name.to_owned(), c);
                    }
                } else {
                    println!(
                        "  [dry-run] Remerge sum '{}' from summands [{}]",
                        sum.name(),
                        summand_names.join(", ")
                    );

                    let mut current_merge = summand_commits.first().cloned();
                    if detect_conflicts && summand_commits.len() >= 2 {
                        let empty_tree = repository.treebuilder(None).and_then(|b| b.write()).and_then(|id| repository.find_tree(id)).ok();
                        let mut first_commit = summand_commits[0].clone();
                        for (i, other_commit) in summand_commits.iter().enumerate().skip(1) {
                            let ancestor_oid = repository
                                .merge_base(first_commit.id(), other_commit.id())
                                .ok();
                            let ancestor_tree = ancestor_oid
                                .and_then(|oid| repository.find_commit(oid).ok())
                                .and_then(|c| c.tree().ok());
                            let ancestor_tree = ancestor_tree.as_ref().or(empty_tree.as_ref()).unwrap();
                            let our_tree = first_commit.tree()?;
                            let their_tree = other_commit.tree()?;

                            let mut merge_opts = MergeOptions::new();
                            merge_opts.patience(true).ignore_whitespace(true);

                            let mut index = repository.merge_trees(
                                ancestor_tree,
                                &our_tree,
                                &their_tree,
                                Some(&mut merge_opts),
                            )?;

                            if index.has_conflicts() {
                                println!(
                                    "    [conflict] Sum '{}': conflict merging summand '{}'",
                                    sum.name(),
                                    summand_names[i]
                                );
                                conflicts_found = true;
                                break;
                            } else {
                                let tree_oid = index.write_tree_to(repository)?;
                                let tree = repository.find_tree(tree_oid)?;
                                let sig = repository.signature().unwrap_or_else(|_| {
                                    git2::Signature::now("DryRun", "dry@run").unwrap()
                                });
                                let msg = get_merge_commit_message(
                                    sum.name(),
                                    &summand_names[0],
                                    summand_names.iter().skip(1).map(|s| s.as_str()),
                                );
                                let new_oid = repository.commit(
                                    None,
                                    &sig,
                                    &sig,
                                    &msg,
                                    &tree,
                                    &[&first_commit, other_commit],
                                )?;
                                first_commit = repository.find_commit(new_oid)?;
                            }
                        }
                        current_merge = Some(first_commit);
                    }

                    if let Some(c) = current_merge {
                        sim_tips.insert(name.to_owned(), c);
                    }
                }
            }
        }
    }

    if conflicts_found {
        println!("Dry run completed: conflicts detected.");
    } else {
        println!("Dry run completed: no conflicts detected.");
    }

    Ok(())
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();
    init_tracing(cli.verbose);

    let repository = cli.git_repository.open()?;

    if cli.cont {
        // old: rebase_continue_git1(repository, &segment_name)
        rebase_segment_continue(&repository)
            .context("failed to continue segment rebase")?;
    } else {
        // fixme: what if SUM?
        match segment_to_continue(&repository) {
            Ok(Some((segment_name, _))) => {
                bail!("rebase underway, must use continue -c {}", segment_name);
            }
            Err(e) => {
                return Err(e).context("Error reading rebase state");
            }
            Ok(None) => {}
        }
    }

    let root = match cli.root_reference {
        Some(r) => r,
        None => repository
            .head()
            .context("failed to resolve HEAD reference")?
            .name()
            .ok_or_else(|| anyhow!("HEAD reference missing name"))?
            .to_owned(),
    };
    Segment::check_name_is_valid(&root)?;

    let root = GitHierarchy::Name(root); // todo: load?

    debug!("root is {}", root.node_identity());

    resolve_reference_names_from_user(&repository, &mut cli.ignore)
        .context("failed to resolve ignore references")?;
    resolve_reference_names_from_user(&repository, &mut cli.skip)
        .context("failed to resolve skip references")?;

    if cli.dry || cli.detect_conflicts {
        rebase_tree_dry(
            &repository,
            root.node_identity().to_owned(),
            &cli.ignore,
            &cli.skip,
            cli.detect_conflicts,
        ).with_context(|| format!("failed dry run for tree starting at '{}'", root.node_identity()))?;
    } else {
        rebase_tree(
            &repository,
            root.node_identity().to_owned(),
            !cli.no_fetch,
            &cli.ignore,
            &cli.skip,
        ).with_context(|| format!("failed to rebase tree starting at '{}'", root.node_identity()))?;

        eprintln!("{}", Colorize::green("Done"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::git_hierarchy::test_utils::{create_commit, TestRepo};

    #[test]
    fn test_remerge_sum_rejects_invalid_summand_name() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit1 = create_commit(repo, "commit 1", &[]);
        let commit2 = create_commit(repo, "commit 2", &[]);
        let merge_commit = create_commit(repo, "merge", &[&commit1, &commit2]);

        let b1 = repo.branch("b1", &commit1, false).unwrap();
        let b2 = repo.branch("b2", &commit2, false).unwrap();

        let refs = [b1.get(), b2.get()];
        let sum = Sum::create(repo, "valid-sum", refs.into_iter(), Some(merge_commit)).unwrap();

        let mut object_map = HashMap::new();
        object_map.insert(
            "refs/heads/b1".to_string(),
            GitHierarchy::Name("-option-inject".to_string()),
        );
        object_map.insert(
            "refs/heads/b2".to_string(),
            GitHierarchy::Reference(b2.into_reference()),
        );

        let res = remerge_sum(repo, &sum, &object_map);
        assert!(matches!(res, Err(RebaseError::WrongHierarchy(_))));
    }

    // marker to avoid merge conflicts

    #[test]
    fn test_fetch_upstream_of_out_of_sync() {
        use ::git_hierarchy::test_utils::{create_commit, TestRepo};

        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let commit1 = create_commit(repo, "commit 1", &[]);
        let commit2 = create_commit(repo, "commit 2", &[&commit1]);

        let mut branch = repo.branch("feature", &commit1, false).unwrap();
        repo.remote("origin", "https://example.com/repo.git").unwrap();

        // Set up a remote tracking reference
        repo.reference("refs/remotes/origin/feature", commit2.id(), true, "test remote").unwrap();
        branch.set_upstream(Some("origin/feature")).unwrap();

        let branch_ref = branch.get();
        // Since local branch is at commit1 and remote is at commit2, they are out of sync.
        let result = fetch_upstream_of(repo, branch_ref);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in sync with upstream"));
    }

    #[test]
    fn test_rebase_tree_dry_uptodate() {
        let temp_repo = TestRepo::new();
        let repo = &temp_repo.repo;

        let commit1 = create_commit(repo, "initial", &[]);
        let main_branch = repo.branch("main", &commit1, true).unwrap();
        let seg = Segment::create(repo, "feature", main_branch.get(), commit1.id(), commit1.id()).unwrap();

        let res = rebase_tree_dry(repo, seg.name().to_string(), &[], &[], false);
        assert!(res.is_ok());
    }

    #[test]
    fn test_rebase_tree_dry_detect_conflicts() {
        let temp_repo = TestRepo::new();
        let repo = &temp_repo.repo;

        // Base commit on main with file1.txt: "1\n2\n3\n"
        let file1_path = temp_repo.path.join("file1.txt");
        std::fs::write(&file1_path, "1\n2\n3\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file1.txt")).unwrap();
        index.write().unwrap();
        let base_commit = temp_repo.create_commit("initial commit", &[]);
        repo.branch("main", &base_commit, true).unwrap();

        // Segment 'feature' with base 'main'
        let main_ref = repo.find_reference("refs/heads/main").unwrap();
        let seg = Segment::create(repo, "feature", &main_ref, base_commit.id(), base_commit.id()).unwrap();

        // Feature commit: file1.txt modified to "1\nA\n2\n3\n"
        std::fs::write(&file1_path, "1\nA\n2\n3\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file1.txt")).unwrap();
        index.write().unwrap();
        let feat_commit = temp_repo.create_commit("change 1", &[&base_commit]);
        repo.reference("refs/heads/feature", feat_commit.id(), true, "update feature").unwrap();

        // Update main branch with conflicting change: file1.txt modified to "1\n2 modified\n3\n"
        std::fs::write(&file1_path, "1\n2 modified\n3\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file1.txt")).unwrap();
        index.write().unwrap();
        let main_commit = temp_repo.create_commit("change 2", &[&base_commit]);
        repo.reference("refs/heads/main", main_commit.id(), true, "update main").unwrap();

        let res = rebase_tree_dry(repo, seg.name().to_string(), &[], &[], true);
        assert!(res.is_ok());

        // Ensure branches/HEAD were not modified by dry run!
        let feature_ref = repo.find_reference("refs/heads/feature").unwrap();
        assert_eq!(feature_ref.target().unwrap(), feat_commit.id());
    }
}

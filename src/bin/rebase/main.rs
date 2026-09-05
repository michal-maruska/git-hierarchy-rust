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

use ::git_hierarchy::base::{checkout_new_head_at, git_same_ref, open_repository, upstream_of,to_branch};
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

use std::path::PathBuf;
use std::process::exit;
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

            git_run(repository, &cmdline)?;
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
            let mut index = repository.index().unwrap();
            if index.has_conflicts() {
                info!("{}: SORRY conflicts detected", line!());
                exit(1);
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

/// Given full git-reference name /refs/remotes/xx/bb return xx and bb
fn extract_remote_name(name: &str) -> Option<(&str, &str)> {
    debug!("extract_remote_name: {:?}", name);
    let rest = name.strip_prefix("refs/remotes/")?;
    rest.split_once('/')
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
        if let Some((mut remote, _remote_branch, remote_branch_name)) = upstream_of(repository, &branch) {

            if git_same_ref(repository, reference, branch.get())? {
                // we might be behind?
                debug!("in sync, so let's fetch & update");
            } else {
                // Check if still in sync, to not lose local changes.
                panic!("{} not in sync with upstream {}; should not update.", name, branch.name().unwrap().unwrap());
                // or merge/rebase.
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
) -> Result<RebaseResult, RebaseError> {
    match node {
        GitHierarchy::Name(_n) => {
            panic!();
        }
        GitHierarchy::Reference(r) => {
            if fetch {
                fetch_upstream_of(repo, r)?; // .expect("fetch failed")
            }
            Ok(RebaseResult::Done)
        }
        GitHierarchy::Segment(segment) => {
            let my_span = span!(Level::INFO, "segment", name = segment.name());
            let _enter = my_span.enter();
            rebase_segment(repo, segment)
        }
        GitHierarchy::Sum(sum) => {
            let _my_span = span!(Level::INFO, "sum", name = sum.name());
            remerge_sum(repo, sum, object_map)
        }
    }
}

fn check_node<'repo>(
    repo: &'repo Repository,
    node: &GitHierarchy<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
) -> Result<(), RebaseError>{
    match node {
        GitHierarchy::Name(_n) => {
            panic!();
        }
        GitHierarchy::Reference(_r) => {
            // no
        }
        GitHierarchy::Segment(segment) => {
            check_segment(repo, segment)?;
        }
        GitHierarchy::Sum(sum) => {
            check_sum(repo, sum, object_map)?;
        }
    }

    Ok(())
}


// whole hierarchy
fn rebase_tree(repository: &Repository,
               root: String,
               fetch: bool,
               ignore: &[String],
               skip: &[String]
) -> Result<(), RebaseError> {
    debug!("find the hierarchy from {}", &root);
    tracing::debug_span!("hierarchy");
    let hierarchy_graph = find_hierarchy(repository, root);

    // verify we can do it:
    debug!("Verify");
    for v in &hierarchy_graph.discovery_order {
        let name = hierarchy_graph.labeled_objects.get(v).unwrap().node_identity();
        debug!(
            "{:?} -> ({:?} / {:?})",
            v,
            name,
            hierarchy_graph.graph
                .node_weight(*hierarchy_graph.labeled_nodes.get(v).unwrap())
                .unwrap()
        );
        if ignore.iter().any(|x| x == name) {
            info!("not checking: {name}");
            continue;
        }
        let vertex = hierarchy_graph.labeled_objects.get(v).unwrap();
        check_node(repository, vertex, &hierarchy_graph.labeled_objects)?
            // with context .expect("nodes should be in correct state");
    }

    debug!("Rebasing");
    for v in &hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph.labeled_objects.get(v).unwrap();
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
    #[arg(long, short = 'g')]
    directory: Option<PathBuf>,

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
    skip: Vec<String>
}

fn main() {
    let mut cli = Cli::parse();
    init_tracing(cli.verbose);

    let repository = open_repository(cli.directory.as_ref()).expect("should find the Git directory");

    if cli.cont {
        // old: rebase_continue_git1(repository, &segment_name)
        rebase_segment_continue(&repository).unwrap();
    } else {
        // fixme: what if SUM?
        match segment_to_continue(&repository) {
            Ok(Some((segment_name, _))) => {
                eprintln!("{} {}", Colorize::bright_magenta("rebase underway, must use continue -c"), segment_name);
                exit(1);
            }
            Err(e) => {
                eprintln!("{}: {:?}", Colorize::red("Error reading rebase state"), e);
                exit(1);
            }
            Ok(None) => {}
        }
    }

    let root = cli.root_reference
        .unwrap_or_else(|| repository.head().unwrap()
                        // if in detached HEAD -- will panic:
                        .name().unwrap().to_owned());

    let root = GitHierarchy::Name(root); // todo: load?

    debug!("root is {}", root.node_identity());

    if !cli.ignore.is_empty() {
        for e in cli.ignore.iter_mut() {
            if let Ok(r) = repository.resolve_reference_from_short_name(e) {
                if let Some(n) = r.name() {
                    *e = n.to_string();
                }
            }
        }
    }

    if !cli.skip.is_empty() {
        for e in cli.skip.iter_mut() {
            if let Ok(r) = repository.resolve_reference_from_short_name(e) {
                if let Some(n) = r.name() {
                    *e = n.to_string();
                }
            }
        }
    }

    if let Err(e) = rebase_tree(&repository,
                                // why?
                                root.node_identity().to_owned(),
                                !cli.no_fetch,
                                &cli.ignore,
                                &cli.skip)
    {
        eprintln!("Failed: {:?}", e); // RebaseError
        exit(-1);
    } else {
        eprintln!("{}",Colorize::green("Done"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_remote_name() {
        assert_eq!(
            extract_remote_name("refs/remotes/origin/main"),
            Some(("origin", "main"))
        );
        assert_eq!(
            extract_remote_name("refs/remotes/upstream/feature/branch"),
            Some(("upstream", "feature/branch"))
        );
        assert_eq!(extract_remote_name("refs/heads/main"), None);
        assert_eq!(extract_remote_name("invalid_ref"), None);
        assert_eq!(extract_remote_name("refs/remotes/no_slash"), None);
    }
}

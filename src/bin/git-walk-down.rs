use clap::Parser;
use git2::{Repository, Reference};
use anyhow::{anyhow, bail, Context, Result};

use colored::Colorize;

use std::collections::HashMap;

use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::utils::{init_tracing, concatenate};
use git_hierarchy::base::{current_branch, upstream_of, to_branch};

use ::git_hierarchy::graph::discover::NodeExpander;
use ::git_hierarchy::graph::discover_pet::find_hierarchy;

#[allow(unused)]
use ::git_hierarchy::git_hierarchy::{GitHierarchy, Segment, Sum, load,
                                     segment_fmt, sum_fmt, plain_ref_fmt};

use ::git_hierarchy::rebase::check_sum;
#[allow(unused)]
use tracing::{debug, info};

/** walk the hierarchy and:
 - visit & display a list of segments/sums.
 - clone
 - replaceInHierarchy ...the base from->to, mapping
*/
#[derive(Parser, Debug)]
#[command(
    version,
    verbatim_doc_comment,
    about = "Walk the poset hierarchy to display, clone, or rebind segment base references"
)]
struct Cli {
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// shorten the information shown
    #[arg(short='s', group = "format")]
    short: bool,

    /// Rebind base references in the hierarchy from <FROM> to <TO>
    #[arg(long, short='r', num_args(2), value_names = ["FROM", "TO"])]
    replace: Vec<String>,

    /// Clone hierarchy nodes with suffix: [SUFFIX] or [SUFFIX_REMOVE] [SUFFIX_ADD]
    #[arg(long, short = 'c', num_args(1..3), value_names = ["SUFFIX"])]
    clone: Vec<String>,

    root_reference: Option<String>,
}


fn list_segment_commits<'repo>(repository: &'repo Repository, segment: &Segment<'repo>) -> Result<()> {
    let walk = segment.iter(repository)
        .with_context(|| format!("failed to initialize revwalk for segment '{}'", segment.name()))?;
    for c in walk {
        let oid = c.context("failed to read commit oid from revwalk")?;
        let commit = repository.find_commit(oid)
            .with_context(|| format!("failed to find commit {}", oid))?;
        let message = commit.summary().unwrap_or("");
        println!("{:?}: {}", oid, message);
    }
    println!();
    Ok(())
}


fn describe_node<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
    brief: bool,
) -> Result<()> {
    debug!("describe_node: {:?}", node.node_identity());

    match node {
        GitHierarchy::Name(n) => {
            bail!("invalid ::Name node variant in describe_node: {}", n);
        }
        GitHierarchy::Reference(r) => {
            if r.is_branch() {
                let branch = to_branch(repository, r);

                if let Some((remote, _branch, name)) = upstream_of(repository, &branch) {
                    println!(
                        "a ref {} => {} {}",
                        plain_ref_fmt(r.name().unwrap_or("")),
                        remote.name().unwrap_or(""),
                        name
                    );
                }
            } else {
                // tag?
                bail!("implement describe_node for this {}", node.node_identity());
            }
        }
        GitHierarchy::Segment(segment) => {
            let base = segment.base(repository);

            let state: colored::ColoredString = if segment.uptodate(repository) {
                "up-to-date".normal()
            } else {
                "need-rebase".bright_red().on_white()
            };
            println!(
                "segment {}: on {}\t{}",
                segment_fmt(segment.name()),
                base.name().unwrap_or(""),
                state
            );

            if !brief {
                list_segment_commits(repository, segment)?;
            }
        }
        GitHierarchy::Sum(sum) => {
            let summands = sum.summands(repository);

            println!("sum {}", sum_fmt(sum.name()));
            if check_sum(repository, sum, object_map).is_err() {
                println!("{}", "needs update".bright_red().on_white());
            }

            if !brief {
                for s in &summands {
                    println!("  {}", s.name().unwrap_or(""));
                }
            }
        }
    }
    Ok(())
}

fn replace_nodes<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    _object_map: &HashMap<String, GitHierarchy<'repo>>,
    remapped: &mut HashMap<String, String>,
) -> Result<()> {
    debug!("{:?}", node.node_identity());

    match node {
        GitHierarchy::Name(n) => {
            bail!("invalid Name node variant in replace_nodes: {}", n);
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap_or(""));
        }
        GitHierarchy::Segment(segment) => {
            let name = segment.reference.borrow().name()
                .ok_or_else(|| anyhow!("segment reference missing name"))?.to_owned();
            if remapped.get(&name).is_some() {
                info!("this segment is itself to be replaced, so ignoring");
                return Ok(());
            }

            let base = segment.base(repository);
            let base_name = base.name().ok_or_else(|| anyhow!("base reference missing name"))?;

            if let Some(replacement) = remapped.get(base_name) {
                debug!("exchange base {}", base_name);
                segment.base.borrow_mut()
                    .symbolic_set_target(replacement, "replacement")
                    .with_context(|| format!("failed to rebind base reference to '{}'", replacement))?;
            }
        }
        GitHierarchy::Sum(sum) => {
            let summands = sum.summands(repository);

            println!("a sum of: ");
            for s in &summands {
                let name = s.name().unwrap_or("");
                println!("{}", name);

                if remapped.get(name).is_some() {
                    println!("Would change the summand {}", name);
                }
            }
        }
    }
    Ok(())
}

fn register_for_replacement<'repo>(
    remapped: &mut HashMap<String, String>,
    from: &Reference<'repo>,
    target: &Reference<'repo>,
) -> Result<()> {
    let name = from.name().ok_or_else(|| anyhow!("source reference missing name"))?.to_owned();
    let target = target.name().ok_or_else(|| anyhow!("target reference missing name"))?.to_owned();
    info!("will replace {} with {}", &name, &target);
    if remapped.insert(name.clone(), target).is_some() {
        bail!("duplicate replacement entry for {}", name);
    }
    debug!("hash: {remapped:?}");
    Ok(())
}


fn clone_node<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    _object_map: &HashMap<String, GitHierarchy<'repo>>,
    remapped: &mut HashMap<String, String>,
    new_name_fn: &dyn Fn(&str) -> String,
) -> Result<()> {
    debug!("clone {:?}", node.node_identity());

    match node {
        GitHierarchy::Name(n) => {
            bail!("invalid Name node variant in clone_node: {}", n);
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap_or(""));
        }
        GitHierarchy::Segment(segment) => {
            let new_name = new_name_fn(segment.name());
            info!("new name is {}", new_name);

            let mut base = segment.base(repository);
            let base_name = base.name().ok_or_else(|| anyhow!("base reference missing name"))?;

            debug!("searching for replace of base {} in {:?}", base_name, remapped);
            if let Some(replacement) = remapped.get(base_name) {
                debug!("found! {replacement}");
                base = repository.find_reference(replacement)
                    .with_context(|| format!("failed to find replacement reference '{}'", replacement))?;
            }
            let target_oid = segment.reference.borrow().target()
                .ok_or_else(|| anyhow!("segment reference missing target commit OID"))?;
            let new_segment = Segment::create(repository,
                                              &new_name,
                                              &base,
                                              segment.start(),
                                              target_oid)
                .with_context(|| format!("failed to create cloned segment '{}'", new_name))?;
            register_for_replacement(remapped,
                                     &segment.reference.borrow(),
                                     &new_segment.reference.borrow())?;
        }
        GitHierarchy::Sum(sum) => {
            let new_name = new_name_fn(sum.name());
            info!("new sum name is {}", new_name);

            let summands = sum.summands(repository);

            println!("a sum of: ");
            let rewritten_summands: Result<Vec<_>> =
                summands.into_iter().map(
                    |s| -> Result<git2::Reference<'repo>>
                    {
                        let name = s.name().unwrap_or("");
                        println!("{}", name);

                        if let Some(replacement) = remapped.get(name) {
                            debug!("found! {replacement}");
                            repository.find_reference(replacement)
                                .with_context(|| format!("failed to find replacement reference '{}'", replacement))
                        } else {
                            Ok(s)
                        }
                    }).collect();
            let rewritten_summands = rewritten_summands?;

            let summands_refs: Vec<_> = rewritten_summands.iter().collect();
            let peel_commit = sum.reference.borrow().peel_to_commit().ok();
            let new_sum = Sum::create(repository,
                                      &new_name,
                                      summands_refs.into_iter(),
                                      peel_commit)
                .with_context(|| format!("failed to create cloned sum '{}'", new_name))?;
            register_for_replacement(remapped,
                                     &sum.reference.borrow(),
                                     &new_sum.reference.borrow())?;
        }
    }
    Ok(())
}


fn walk_down<F>(repository: &Repository, root: &str, mut process: F) -> Result<()>
where
    F: for<'repo, 'a> FnMut(
        &'repo git2::Repository,
        &GitHierarchy<'repo>,
        &'a HashMap<String, GitHierarchy<'repo>>,
    ) -> Result<()>,
{
    let hierarchy_graph = find_hierarchy(repository, root.to_owned());

    for v in hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph.labeled_objects.get(&v)
            .ok_or_else(|| anyhow!("hierarchy vertex not found for '{}'", v))?;
        process(repository, vertex, &hierarchy_graph.labeled_objects)
            .with_context(|| format!("failed processing node '{}'", vertex.node_identity()))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let repository = cli.git_repository.open()?;
    if !cli.replace.is_empty() {
        for r in &cli.replace {
            Segment::check_name_is_valid(r)?;
        }
        if cli.root_reference.is_none() {
            eprintln!("when --replace is used, the top must be stated ... {}",
                      current_branch(&repository).unwrap_or_default());
            bail!("root reference not specified when using --replace");
        }
    }

    if !cli.clone.is_empty() {
        for c in &cli.clone {
            Segment::check_name_is_valid(c)?;
        }
    }

    let root = match cli.root_reference {
        Some(r) => r,
        None => {
            let head = current_branch(&repository)
                .ok_or_else(|| anyhow!("no current branch chosen"))?;
            info!("Start from the HEAD = {}", head);
            head
        }
    };
    Segment::check_name_is_valid(&root)?;

    info!("Start from the HEAD = {}", &root);

    // clone.
    if !cli.clone.is_empty() {

        let mut remapped = HashMap::new();
        info!("cloning {:?}", cli.clone);
        let new_name: Box<dyn Fn(&str) -> String> =
            if cli.clone.len() == 1 {
                debug!("Will only append suffix {}", cli.clone[0]);
                let suffix = cli.clone[0].clone();
                Box::new(
                    move |x: &str|
                    concatenate(x, &suffix))
            } else {
                let suffix_remove = cli.clone[0].clone();
                let suffix_add = cli.clone[1].clone();
                Box::new(
                    move |x : &str|
                    concatenate(
                        x.strip_suffix(&suffix_remove).unwrap_or(x),
                        &suffix_add))
            };

        walk_down(&repository, &root,
                  |repository, node, object_map| {
                      clone_node(repository, node, object_map, &mut remapped,
                                 &new_name)
                  })?;
    };

    if !cli.replace.is_empty() {
        if cli.replace.len() < 2 {
            bail!("replace requires 2 reference parameters");
        }
        info!("Replacing");
        let mut remapped = HashMap::new();

        let from = repository.resolve_reference_from_short_name(&cli.replace[0])
            .with_context(|| format!("failed to resolve reference '{}'", cli.replace[0]))?;
        let target = repository.resolve_reference_from_short_name(&cli.replace[1])
            .with_context(|| format!("failed to resolve reference '{}'", cli.replace[1]))?;
        register_for_replacement(&mut remapped, &from, &target)?;

        walk_down(&repository, &root, |repository, node, object_map| {
            replace_nodes(repository, node, object_map, &mut remapped)
        })?;
    } else {
        walk_down(&repository, &root,
                  |repository, node, _object_map|
                  describe_node(repository, node, _object_map, cli.short))?;
    }
    Ok(())
}

use clap::Parser;
use git2::{Repository,Reference};

use colored::Colorize;

use std::collections::HashMap;

use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::base::current_branch;
use git_hierarchy::utils::{init_tracing,concatenate};
use git_hierarchy::base::{upstream_of, to_branch};
/*
 note: ambiguous because of a conflict between a name from a glob
       import and an outer scope during import or macro resolution
   = note: `git_hierarchy` could refer to a crate passed with `--extern`
   = help: use `::git_hierarchy` to refer to this crate unambiguously
*/

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
#[command(version,verbatim_doc_comment)]
struct Cli {
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// shorten the information shown
    #[arg(short='s', group = "format")]
    short: bool,

/*
    /// resolve the head
    #[arg(short='G', group = "format")]
    resolve: bool,
*/
    #[arg(long, short='r', num_args(2))]
    replace: Vec<String>,

    // suffix, or  suffix-remove, suffix-add
    #[arg(long, short = 'c', num_args(1..3))]
    clone: Vec<String>,
    // Bug:

    // fixme: here we can use -- to end the vector?
    root_reference: Option<String>,
}


fn list_segment_commits<'repo>(repository: &'repo Repository, segment: &Segment<'repo>) -> Result<(), git2::Error> {
    let walk = segment.iter(repository)?;
    for c in walk {
        let oid = c?;
        let commit = repository.find_commit(oid)?;
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
    // _remapped : HashMap<String, String>,
    brief: bool,
) -> Result<(), git2::Error> {
    debug!("describe_node: {:?}", node.node_identity());
    // let = false;

    match node {
        GitHierarchy::Name(_n) => {
            return Err(git2::Error::from_str("unexpected GitHierarchy::Name node"));
        }
        GitHierarchy::Reference(r) => {
            // say the upstream:
            if r.is_branch() { // and we know it's not Segment/Sum, right?
                let branch = to_branch(repository, r);

                if let Some((_remote, _branch , name)) = upstream_of(repository, &branch) {
                    println!("a ref {} => {} {}", plain_ref_fmt(r.name().unwrap_or("")),
                             _remote.name().unwrap_or(""),
                             name);
                }
            } else {
                // tag?
            }
        }
        GitHierarchy::Segment(segment) => {
            let base = segment.base(repository);

            let state : colored::ColoredString =
                if segment.uptodate(repository) {
                    "up-to-date".normal()
                    // how did I get this? use Trait and get the str type extended?
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
) -> Result<(), git2::Error> {
    debug!(
        "{:?}",
        // object_map.get(&v).unwrap()
        node.node_identity(),
        // object_map
        // graph.node_weight(hash_to_graph.get(node).unwrap().clone()).unwrap()
    );

    match node {
        GitHierarchy::Name(_n) => {
            return Err(git2::Error::from_str("unexpected GitHierarchy::Name node"));
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap_or(""));
        }
        GitHierarchy::Segment(segment) => {
            // if segment itself in replace ... ignore it.
            if let Some(ref_name) = segment.reference.borrow().name() {
                if remapped.get(ref_name).is_some() {
                    info!("this segment is itself to be replaced, so ignoring");
                    return Ok(());
                }
            }

            let base = segment.base(repository);
            if let Some(base_name) = base.name() {
                if let Some(replacement) = remapped.get(base_name) {
                    debug!("exchange base {}", base_name);
                    segment.base.borrow_mut().symbolic_set_target(replacement, "replacement")?;
                }
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
)
{
    let name = match from.name() {
        Some(n) => n.to_owned(),
        None => return,
    };
    let target_name = match target.name() {
        Some(n) => n.to_owned(),
        None => return,
    };
    info!("will replace {} with {}", &name, &target_name);
    remapped.insert(name, target_name);
    debug!("hash: {remapped:?}");
}


fn clone_node<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    _object_map: &HashMap<String, GitHierarchy<'repo>>,
    remapped: &mut HashMap<String, String>,
    new_name_fn: &dyn Fn(&str) -> String,
) -> Result<(), git2::Error>
{
    debug!("clone {:?}", node.node_identity(),);

    // so I create, and put into remapped!
    match node {
        GitHierarchy::Name(_n) => {
            return Err(git2::Error::from_str("unexpected GitHierarchy::Name node"));
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap_or(""));
        }
        GitHierarchy::Segment(segment) => {
            // if segment itself in replace ... ignore it.
            let new_name = new_name_fn(segment.name());
            info!("new name is {}", new_name);

            // get the base:
            let mut base = segment.base(repository);
            if let Some(base_name) = base.name() {
                debug!("searching for replace of base {} in {:?}", base_name, remapped);
                if let Some(replacement) = remapped.get(base_name) {
                    debug!("found! {replacement}");
                    base = repository.find_reference(replacement)?;
                }
            }
            let target_oid = segment.reference.borrow().target().ok_or_else(|| {
                git2::Error::from_str("segment reference missing target commit OID")
            })?;
            let new_segment = Segment::create(repository,
                                              &new_name,
                                              &base,
                                              segment.start(),
                                              target_oid)?;
            register_for_replacement(remapped,
                                     &segment.reference.borrow(),
                                     &new_segment.reference.borrow());
        }
        GitHierarchy::Sum(sum) => {
            let new_name = new_name_fn(sum.name());
            info!("new sum name is {}", new_name);

            let summands = sum.summands(repository);

            println!("a sum of: ");
            let rewritten_summands: Result<Vec<_>, _> =
                summands.into_iter().map(
                    |s| -> Result<git2::Reference<'repo>, git2::Error>
                    {
                        let name = s.name().unwrap_or("");
                        println!("{}", name);

                        if let Some(replacement) = remapped.get(name) {
                            debug!("found! {replacement}");
                            repository.find_reference(replacement)
                        } else {
                            Ok(s)
                        }
                    }).collect();
            let rewritten_summands = rewritten_summands?;

            let summands_refs : Vec<_> = rewritten_summands.iter().collect();

            let peel_commit = sum.reference.borrow().peel_to_commit().ok();
            let new_sum = Sum::create(repository,
                                      &new_name,
                                      summands_refs.into_iter(),
                                      peel_commit)?;
            register_for_replacement(remapped,
                                     &sum.reference.borrow(),
                                     &new_sum.reference.borrow()
            );
        }
    }
    Ok(())
}


fn walk_down<F>(repository: &Repository, root: &str, mut process: F) -> Result<(), git2::Error>
where
    F: for<'repo, 'a> FnMut(
    &'repo git2::Repository,
    &GitHierarchy<'repo>,
    &'a HashMap<String, GitHierarchy<'repo>>,
) -> Result<(), git2::Error>
{
    let hierarchy_graph = find_hierarchy(repository, root.to_owned());

    for v in hierarchy_graph.discovery_order {
        if let Some(vertex) = hierarchy_graph.labeled_objects.get(&v) {
            process(repository,
                    vertex,
                    &hierarchy_graph.labeled_objects)?;
        }
    }
    Ok(())
}

fn main() -> Result<(), git2::Error> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let repository = cli.git_repository.open()?;
    if !cli.replace.is_empty() {
        for r in &cli.replace {
            Segment::check_name_is_valid(r)?;
        }
        // also, in this case I don't start *implicitly* by HEAD.
        if cli.root_reference.is_none() {
            eprintln!("when --replace is used, the top must be stated ... {}",
                      current_branch(&repository).unwrap_or_default());
            return Err(git2::Error::from_str("root not specified"));
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
            let head = current_branch(&repository).ok_or_else(|| git2::Error::from_str("no current branch chosen"))?;
            info!("Start from the HEAD = {}", head);
            head
        }};
    Segment::check_name_is_valid(&root)?;

    info!("Start from the HEAD = {}", &root);

    // clone.
    if !cli.clone.is_empty() {

        let mut remapped = HashMap::new();
        info!("cloning {:?}", cli.clone);
        let new_name : Box<dyn Fn(&str) -> String> =
            if cli.clone.len() == 1 {
                debug!("Will only append suffix {}", cli.clone[0]);
                let suffix = cli.clone[0].clone();
                Box::new(
                    move |x : &str|
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

    // and possibly *then* rename?
    if !cli.replace.is_empty() {
        if cli.replace.len() < 2 {
            return Err(git2::Error::from_str("replace requires 2 reference parameters"));
        }
        info!("Replacing");
        // resolve them...
        let mut remapped = HashMap::new();

        let from = repository.resolve_reference_from_short_name(&cli.replace[0])?;
        let target = repository.resolve_reference_from_short_name(&cli.replace[1])?;
        register_for_replacement(&mut remapped, &from, &target);
        // move object_map ?
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

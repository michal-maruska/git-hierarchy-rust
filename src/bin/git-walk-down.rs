//
// - clone
// - replaceInHierarchy ...the base from->to, mapping

use clap::Parser;
use git2::{Repository,Reference};

use colored::Colorize;

use std::collections::HashMap;
use std::path::PathBuf;

use git_hierarchy::utils::{init_tracing,concatenate};
use git_hierarchy::base::{open_repository};
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


fn list_segment_commits<'repo>(repository: &'repo Repository, segment: &Segment<'repo>) {
    let walk = segment.iter(repository).unwrap();
    for c in walk {
        let oid = c.unwrap();
        let commit = repository.find_commit(oid).unwrap();
        let message = commit.summary().unwrap();
        println!("{:?}: {}", oid, message);
    }
    println!();
}


fn describe_node<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    object_map: &HashMap<String, GitHierarchy<'repo>>,
    // _remapped : HashMap<String, String>,
    brief: bool,
) {
    debug!("describe_node: {:?}", node.node_identity());
    // let = false;

    match node {
        GitHierarchy::Name(_n) => {
            panic!();
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", plain_ref_fmt(r.name().unwrap()));
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
                base.name().unwrap(),
                state
            );

            if !brief {
                list_segment_commits(repository, segment);
            }
        }
        GitHierarchy::Sum(sum) => {
            let summands = sum.summands(repository);

            println!("sum {}", sum_fmt(sum.name()));
            if let Err(_) = check_sum(repository, sum, object_map) {
                println!("{}", "needs update".bright_red().on_white());
            }

            if !brief {
                for s in &summands {
                    println!("  {}", s.name().unwrap());
                }
            }
        }
    }
}

fn replace_nodes<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    _object_map: &HashMap<String, GitHierarchy<'repo>>,
    remapped: &mut HashMap<String, String>,
) {
    debug!(
        "{:?}",
        // object_map.get(&v).unwrap()
        node.node_identity(),
        // object_map
        // graph.node_weight(hash_to_graph.get(node).unwrap().clone()).unwrap()
    );

    match node {
        GitHierarchy::Name(_n) => {
            panic!();
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap());
        }
        GitHierarchy::Segment(segment) => {
            // if segment itself in replace ... ignore it.
            if remapped.get(segment.reference.borrow().name().unwrap()).is_some() {
                info!("this segment is itself to be replaced, so ignoring");
                return;
            }

            let base = segment.base(repository);
            let base_name = base.name().unwrap();

            if let Some(replacement) = remapped.get(base_name) {
                debug!("exchange base {}", base_name);
                segment.base.borrow_mut().symbolic_set_target(replacement, "replacement")
                    .expect("should be possible to change Base symbolic reference");
            }
        }
        GitHierarchy::Sum(sum) => {
            let summands = sum.summands(repository);

            println!("a sum of: ");
            for s in &summands {
                let name = s.name().unwrap();
                println!("{}", name);

                if remapped.get(name).is_some() {
                    println!("Would change the summand {}", name);
                }
            }
        }
    }
}

fn register_for_replacement<'repo>(
    remapped: &mut HashMap<String, String>,
    from: &Reference<'repo>,
    target: &Reference<'repo>,
)
{
    let name = from.name().unwrap().to_owned();
    let target = target.name().unwrap().to_owned();
    info!("will replace {} with {}", &name, &target);
    remapped.insert(name, target).map(|_ : String| -> Option<String> {panic!("double")});
    debug!("hash: {remapped:?}");
}


fn clone_node<'repo>(
    repository: &'repo Repository,
    node: &GitHierarchy<'repo>,
    _object_map: &HashMap<String, GitHierarchy<'repo>>,
    remapped: &mut HashMap<String, String>,
    new_name_fn: &dyn Fn(&str) -> String,
)
{
    debug!("clone {:?}", node.node_identity(),);

    // so I create, and put into remapped!
    match node {
        GitHierarchy::Name(_n) => {
            panic!();
        }
        GitHierarchy::Reference(r) => {
            println!("a ref {}", r.name().unwrap());
        }
        GitHierarchy::Segment(segment) => {
            // if segment itself in replace ... ignore it.
            let new_name = new_name_fn(segment.name());
            info!("new name is {}", new_name);

            // get the base:
            // ReferenceType::Symbolic
            let mut base = segment.base(repository);
            let base_name = base.name().unwrap();

            debug!("searching for replace of base {} in {:?}", base_name, remapped);
            if let Some(replacement) = remapped.get(base_name) {
                debug!("found! {replacement}");
                base = repository.find_reference(replacement).unwrap();
            }
            let new_segment = Segment::create(repository,
                                              &new_name,
                                              &base, //  fixme!
                                              segment.start(),
                                              segment.reference.borrow().target().unwrap())
                .unwrap();

            // fixme: I need full ref name:
            register_for_replacement(remapped,
                                     &segment.reference.borrow(),
                                     &new_segment.reference.borrow());
        }
        GitHierarchy::Sum(sum) => {
            let new_name = new_name_fn(sum.name());
            info!("new sum name is {}", new_name);

            let summands = sum.summands(repository);
            // we need references, so the References are not moved/consumed


            println!("a sum of: ");
            // extract the names? full ref names
            // let summand_names =
            let rewritten_summands : Vec<_> =
                summands.into_iter().map(
                    |s|
                    {
                        let name = s.name().unwrap();
                        println!("{}", name);

                        if let Some(replacement) = remapped.get(name) {
                            debug!("found! {replacement}");
                            // println!("Would change the summand {}", name);
                            repository.find_reference(replacement).unwrap()
                        } else {
                            s
                        }
                    }).collect();

            let summands_refs : Vec<_> = rewritten_summands.iter().collect();

            let new_sum = Sum::create(repository,
                                      &new_name,
                                      summands_refs.into_iter(),
                                      Some(sum.reference.borrow().peel_to_commit().unwrap())).unwrap();
            register_for_replacement(remapped,
                                     &sum.reference.borrow(),
                                     &new_sum.reference.borrow()
            );
        }
    }
}


fn walk_down<F>(repository: &Repository, root: &str, mut process: F)
where
    F: for<'repo, 'a> FnMut(
    &'repo git2::Repository,
    &GitHierarchy<'repo>,
    &'a HashMap<String, GitHierarchy<'repo>>,
)
{
    let hierarchy_graph = find_hierarchy(repository, root.to_owned());

    // convert the gh objects?
    for v in hierarchy_graph.discovery_order {
        let vertex = hierarchy_graph.labeled_objects.get(&v).unwrap();
        process(repository,
                vertex,
                &hierarchy_graph.labeled_objects);
    }
}

/// walk the hierarchy
/// - visit & display a list of segments/sums.
#[derive(Parser, Debug)]
#[command(version)]
struct Cli {
    #[arg(long, short='g')]
    directory: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[arg(short='s')]
    short: bool,

    #[arg(long, short = 'r', num_args(2))]
    replace: Vec<String>,

    // suffix, or  suffix-remove, suffix-add
    #[arg(long, short = 'c', num_args(1..3))]
    clone: Vec<String>,

    // fixme: here we can use -- to end the vector?
    root_reference: Option<String>,
}

// detached head? -> None
fn current_branch(repository: &'_ Repository) -> Option<String> {
    let head = repository.head().unwrap();

    if head.is_branch() {
        let head = head.name().unwrap().to_owned();

        // let head = repo.head().unwrap().name().unwrap();
        //                       ^^^
        // creates a temporary value which is freed while still in use
        // what? that is no more temporary?
        // let head = repo.head().unwrap();
        // let head = head.name().unwrap().to_owned();
        Some(head.to_owned())
    } else {
        None
    }
}

fn main() {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let repository = open_repository(cli.directory.as_ref()).unwrap();
    if !cli.replace.is_empty() {
        // also, in this case I don't start *implicitly* by HEAD.
        if cli.root_reference.is_none() {
            eprintln!("when --replace is used, the top must be stated ... {}",
                      current_branch(&repository).unwrap());
            std::process::exit(1);
        }
    }

    let root = cli
        .root_reference
        .unwrap_or_else(|| {
            let head = current_branch(&repository).expect("no current branch chosen");
            info!("Start from the HEAD = {}", head);

            head
        });

    info!("Start from the HEAD = {}", &root);

    // clone.
    if !cli.clone.is_empty() {

        let mut remapped = HashMap::new();
        info!("cloning {:?}", cli.clone);
        // todo: drop suffix, add a new one.
        // fn(&str)->String =
        let new_name : Box<dyn Fn(&str) -> String> =
            if cli.clone.len() == 1 {
                debug!("Will only append suffix {}", cli.clone[0]);
                Box::new(
                    move |x : &str|
                    concatenate(x, &cli.clone[0]))
            } else {
                Box::new(
                move |x : &str|
                concatenate(
                    x.strip_suffix(&cli.clone[0]).unwrap(),
                    &cli.clone[1]))
            };

        walk_down(&repository, &root,
                  |repository, node, object_map| {
                      clone_node(repository, node, object_map, &mut remapped,
                                 &new_name)
                  });
    };

    // and possibly *then* rename?
    if !cli.replace.is_empty() {
        info!("Replacing");
        // resolve them...
        let mut remapped = HashMap::new();

        let from = repository.resolve_reference_from_short_name(&cli.replace[0]).unwrap();
        let target = repository.resolve_reference_from_short_name(&cli.replace[1]).unwrap();
        register_for_replacement(&mut remapped, &from, &target);
        // move object_map ?
        walk_down(&repository, &root, |repository, node, object_map| {
            replace_nodes(repository, node, object_map, &mut remapped)
        });
    } else {
        walk_down(&repository, &root,
                  |repository, node, _object_map|
                  describe_node(repository, node, _object_map, cli.short));
    }
}

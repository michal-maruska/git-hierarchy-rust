use std::path::PathBuf;
use clap::{Parser,Subcommand,CommandFactory,FromArgMatches};
use git_hierarchy::base::is_linear_ancestor;
use git2::{Repository,Oid}; // ,build::CheckoutBuilder

#[allow(unused_imports)]
use git_hierarchy::git_hierarchy::{GitHierarchy,Segment,segments,load,
                                   segment_fmt,
};

use tracing::debug;

/// Operate on segments or 1 segment
#[derive(Parser)] // Debug
// about ... Description from Cargo.toml
#[command(version, long_about = None)]
#[command(subcommand_negates_reqs = true)]
#[command(args_conflicts_with_subcommands = true)] // positional arguments
// ^^ this means that Factory produces command, and then ^^ those are called on it?
struct Cli {

    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[command(subcommand)]
    #[command(name="subcommand")]
    // expand shows:
    // .subcommand_required(false)
    // .arg_required_else_help(false);
    command: Option<Commands>,

    define_or_show_args: Option<Vec<String>>,
}


#[derive(clap::Args)]
#[command(name="git", about = None, long_about = None)]
struct ClapGitRepo {
    #[arg(long, short='g')]
    #[arg(global=true)]
    // why option? b/c otherwise .required(!has_default)
    directory: Option<PathBuf>,
}


#[derive(Subcommand)]
enum Commands {
    /// List all segments
    List(ListArgs),
    /// Reposition the Start of a segment
    Restart(RestartArgs),
    /// Reposition the Base of a segment
    Update(RebaseArgs),
    /// Delete a segment
    Delete(DeleteCmd),
    #[command(name="define", version, long_about = None,long_flag("define"),short_flag('D'))]
    /// Define a new segment
    Define(DefineArgs),

    // Command git-hierarchy: command name `define` is duplicated
    //       define vvvvv
    #[command(name="create", version, long_about = None,long_flag("create"),short_flag('c'))]
    /// Create a new segment, and checkout it
    Create(DefineArgs),
}


/// listing all segments
#[derive(clap::Args)]
// why do I have this, and not #[arg()]?
#[command(version, about, long_about = None,long_flag("list"),short_flag('l'))]
#[command(name="list")]
struct ListArgs {
    #[arg(long, short,group = "format")]
    short: bool,

    // inverse?
    #[arg(long, short,group = "format")]
    // action = clap::SetTrue
    full: bool,

    #[arg(long, short='p')]
    diff: bool,

    name: Option<String>,
}



#[derive(clap::Args)]
#[command(version, about, long_about = None,long_flag("restart"),short_flag('r'))]
struct RestartArgs {
    segment_name: String,
    commit: String,
}

#[derive(clap::Args)]
#[command(version, about, long_about = None,long_flag("update"),short_flag('u'))]
struct RebaseArgs {
    #[arg(long="base", short='b')]
    rebase: bool,
    segment_name: String,
    new_base: String,
}


#[derive(clap::Args)]
#[command(version, about, long_about = None,long_flag("delete"),short_flag('d'))]
struct DeleteCmd {
    segment_name: String,
}

// I want this default.... can I flatten it in?
#[derive(clap::Args)]
#[allow(unused_variables)]
// so for Args I can have command? Does it call it during the augment_args(command) call?
struct DefineArgs {
    #[arg(long, short)] // note -c is this command
    checkout: bool,

    segment_name: String,
    base: String,
    start: Option<String>, // default to @base
    head: Option<String>,
}

fn resolve_user_commit(repository: &Repository, input: &str) -> Option<Oid> {
    // either:
    if let Ok(sha) = Oid::from_str(input) {
        if let Ok(commit) = repository.find_commit(sha) {
            Some(commit.id())
        } else {
            debug!("couldn't find the commit {}", sha);
            None
        }
    } else if let Ok(reference) = repository.resolve_reference_from_short_name(input) {
        // refname_to_id
        Some(reference.target().unwrap())
    } else {
        debug!("couldn't find reference {}", input);
        None
    }
}

fn define<'repo> (repository: &'repo Repository, args: &DefineArgs) -> Result<Segment<'repo>, git2::Error>
{
    let base = repository.resolve_reference_from_short_name(&args.base)?;

    // no: either ref or sha
    let start = if let Some(s) = &args.start {
        // note: as_ref().unwrap()  vs unwrap().as_ref() ...
        resolve_user_commit(repository, s).unwrap()
    } else {
        base.target().unwrap()
    };

    let head =
        args.head.as_ref().map_or(
            start,
            |x|
            resolve_user_commit(repository, x).expect("input must be valid")
        );

    let res = Segment::create(repository, &args.segment_name, &base, start, head);

    println!("create {} in {:?}", args.segment_name, repository.path());
    println!("base = {}, start {} = {}", base.name().unwrap(), start, head);
    res
}

fn delete(repository: &Repository, args: &DeleteCmd) {
    // check sums above
    // segments based on it.
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.segment_name).unwrap();
    if let GitHierarchy::Segment(mut segment) = gh {
        println!("Delete {} in {:?}", args.segment_name, repository.path());

        segment.base.borrow_mut().delete().unwrap();
        segment._start.delete().unwrap();
        segment.reference.borrow_mut().delete().unwrap();
    }
}

// see list_segment in git-walk-down.rs
fn describe(repository: &Repository, segment_name: &str) {

    let gh = git_hierarchy::git_hierarchy::load(repository, segment_name).unwrap();
    if let GitHierarchy::Segment(segment) = gh {
        println!("Segment {} in {:?}", segment_fmt(segment_name), repository.path());

        // todo: drop the refs/
        println!("Base {}", segment.base(repository).name().unwrap());
        println!("Start {} lenght {} {}", segment.start(),
                 segment.iter(repository).unwrap().count(),
                 if segment.uptodate(repository) { "clean" } else { "dirty"}
        );
        // check if start is ancestor !!!
        if !is_linear_ancestor(repository,
            segment.start(),
            segment.reference.borrow().peel_to_commit().unwrap().id()
        ).unwrap() {
            // quite an error!
            panic!("Start is not ancestor!");
            // println!("Start is not ancestor!");
        }

        // uptodate?
        // fixme: if not ancestor, is this intended?
        for oid in segment.iter(repository).unwrap() {
            let oid = oid.unwrap();
            let commit = repository.find_commit(oid).unwrap();
            println!("{}: {}", oid, commit.summary().unwrap());
        }
    } else {
        println!("Segment {} does not exist", segment_fmt(segment_name));
    }
}

fn list_segments(repository: &Repository) {
    let ref_iterator = segments(repository);

    for r in ref_iterator {
        println!("{}", segment_fmt(&r));
    }
}


fn main() -> Result<(), Box<dyn std::error::Error>> {

    let clip =
        if true {
            let mut cli = Cli::command();
            //
            cli = cli.subcommand_negates_reqs(true);

            // get_matches, -> ArgMatches
            // Parser trait ... FromArgMatches ...

            if false {
                for i in cli.get_opts() {
                    println!("option {i:?}");
                    // -> impl Iterator<Item = &Arg>
                }
            }

            let mut matches = cli.get_matches();
            // clip = Cli::parse();

            // this fails... MissingRequiredArgument b/c define_args
            Cli::from_arg_matches_mut(&mut matches).expect("assignment failed")
        } else {
            Cli::parse()
        };

    tracing_subscriber::fmt()
        .with_max_level(clip.verbosity)
        .init();

    /*
    let args: Vec<String> = env::args().collect();
    */

    let repository = match clip.git_repository.directory {
        None => Repository::open_from_env().expect("failed to find Git repository"),
        Some(dir) => Repository::open(dir).expect("failed to find Git repository"),
    };

    // this is an associated function, not a method
    if let Some(command) = clip.command {
        match command {
            Commands::List(_args) => {
                list_segments(&repository);
            }
            Commands::Restart(args) => {
                let gh = git_hierarchy::git_hierarchy::load(&repository, &args.segment_name).unwrap();
                if let GitHierarchy::Segment(segment) = gh {
                    let oid =
                        resolve_user_commit(&repository,
                                            args.commit.as_ref())
                        .unwrap();
                    println!("restart from {} {}", args.commit, oid);
                    segment.set_start(&repository, oid);
                }

            },
            Commands::Update(args) => {
                let gh = git_hierarchy::git_hierarchy::load(&repository, &args.segment_name).unwrap();
                if let GitHierarchy::Segment(segment) = gh {
                    let new_base = repository.resolve_reference_from_short_name(&args.new_base)
                        .expect("new base should exist");
                    println!("rebase from {} -> {} {}", args.new_base,
                             new_base.name().unwrap(),
                             if args.rebase {"immediately"} else {""});
                    segment.set_base(&repository, &new_base);
                }
            },
            Commands::Delete(args) => {
                delete(&repository, &args);
            },
            Commands::Create(args) => {
                // checkout immediate
                let seg = define(&repository, &args).expect("failed to define new segment");

                // try to switch
                println!("should checkout now");
                repository.set_head(seg.reference.borrow().name().unwrap()).expect("should set HEAD");
                repository.checkout_head(None).expect("should checkout");
            }
            Commands::Define(args) => {
                define(&repository, &args).expect("failed to define new segment");
            },
        }
    } else if let Some(args) = clip.define_or_show_args {
        if args.is_empty() {
            unreachable!("cannot be Some, and empty vector");
        } else if args.len() == 1 {
            describe(&repository, &args[0]);
        } else {
            // convert....
            let def = DefineArgs {
                checkout : false, // to control this, use the -D/define command.
                // cannot move out of index of `Vec<std::string::String>`
                // so? swap? borrow_mut
                segment_name : args[0].clone(),
                base: args[1].clone(),
                start: if args.len() > 2 {Some(args[2].clone())} else {None},
                head: if args.len() > 3 {Some(args[3].clone())} else {None},
            };
            define(&repository, &def).unwrap();
        }
    } else {
        list_segments(&repository);
    }
    // else nothing. Or list?
    // return Err(error.into());
    Ok(())
}

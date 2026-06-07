use std::path::PathBuf;
use std::process::exit;
use clap::{Parser,Subcommand};
use git2::{Repository,Reference,Oid};
use colored::Colorize;

#[allow(unused_imports)]
use git_hierarchy::git_hierarchy::{GitHierarchy,Sum,load,sums, sum_fmt};


#[allow(unused)]
use tracing::{debug,info,error};

/// Manage Sum information -- merge definitions
#[derive(Parser)]
#[command(version, long_about = None)] // how to use the comment above?
#[command(subcommand_negates_reqs = true)]
#[command(args_conflicts_with_subcommands = true)] // positional arguments
// ^^ this means that Factory produces command, and then ^^ those are called on it?
struct Cli {
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    // move elsewhere
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[command(subcommand)]
    #[command(name="subcommand")]
    command: Option<Commands>,

    define_or_show_args: Option<Vec<String>>,
}

#[derive(clap::Args)]
#[command(name="git", about = None, long_about = None)]
struct ClapGitRepo {
    #[arg(long, short='g')]
    #[arg(global=true)]
    directory: Option<PathBuf>,
}


#[derive(Subcommand)]
enum Commands {
    #[command(name="list", version, about, long_about = None,
              long_flag("list"),short_flag('l'))]
    List(ListArgs),

    #[command(name="delete", version, about, long_about = None,
              long_flag("delete"),short_flag('d'))]
    Delete(DeleteCmd),

    #[command(name="define", version, about, long_about = None,long_flag("define"),short_flag('D'))]
    Define(DefineArgs),

    #[command(name="show", version, about, long_about = None,long_flag("show"),short_flag('s'))]
    Show(ShowArgs),

    #[command(name="add", version, about, long_about = None,long_flag("add"),short_flag('a'))]
    Add(AddArgs),

    // #[command(name="remove", version, about, long_about = None,long_flag("remove"),short_flag('r'))]
    // Remove(RemoveArgs),
}

#[derive(clap::Args)]
struct ShowArgs {
    name: String,
}

#[derive(clap::Args)]
struct AddArgs {
    name: String,
    summands: Vec<String>,
}

#[derive(clap::Args)]
struct RemoveArgs {
    name: String,
    summands: Vec<String>,
}


#[derive(clap::Args)]
// why do I have this, and not #[arg()]?
struct DefineArgs
{
    // run-time error to use "-h"
    #[arg(long, short ='H')]
    head: Option<String>,

    name: String,
    components: Vec<String>,
}

/// listing all sums
#[derive(clap::Args)]
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
#[command(version, about, long_about = None,long_flag("delete"),short_flag('d'))]
struct DeleteCmd {
    sum_name: String,
}

// fn take<>(x: impl IntoIterator<Item=&'a T>)
fn define_sum<'repo,'a, T: AsRef<str> + 'a>(repository: &'repo Repository,
                                            name: &str,
                                            summands: &[T],
                                            hint: Option<T>) {
    let sumrefs : Vec<Reference>
        = summands.iter().map(|x| {
            repository.resolve_reference_from_short_name(x.as_ref()).unwrap()
        }).collect();

    let mut hint_head_oid = None;

    if let Some(s) = hint {
        // resolve:
        if let Ok(sha) = Oid::from_str(s.as_ref()) {
            if let Ok(commit) = repository.find_commit(sha) {
                hint_head_oid = Some(commit); // .id()
            } else {
                debug!("couldn't resolve {}", sha)
            }
        } else {
            debug!("not a valid commit id {}", s.as_ref());
            // hint.map(|x| repository.resolve_reference_from_short_name(x.as_ref()).unwrap()),
            // symbolic ...
        }
    }

    let sum = Sum::create(
        repository,
        name,
        sumrefs.iter(),
        hint_head_oid
    );

    if sum.is_err() {
        eprintln!("{}",Colorize::green("failed to create sum"));
        exit(1);
    }
}

fn delete_sum(repository: &Repository, args: &DeleteCmd) {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.sum_name).unwrap();
    if let GitHierarchy::Sum(sum) = gh {
        info!("deleting {}", args.sum_name);
        // drop all summands
        sum.reference.borrow_mut().delete().unwrap();
        for mut summand in sum.summands { // (repository)
            summand.delete().expect("should be able to drop summand reference");
            // sum.reference.borrow_mut().delete();
        }
    }
}

fn main()
{
    let clip = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(clip.verbosity)
        .init();

    let repository = match clip.git_repository.directory {
        None => Repository::open_from_env().expect("failed to find Git repository"),
        Some(dir) => Repository::open(dir).expect("failed to find Git repository"),
    };


    if let Some(command) = clip.command {
        match command {
            Commands::List(_args) => {
                list_sums(&repository);
            }
            Commands::Define(args) => {
                define_sum(&repository,
                           &args.name,
                           &args.components,
                           args.head);
            }
            Commands::Delete(args) => {
                delete_sum(&repository, &args);
            }
            Commands::Show(args) => {
                describe_sum(&repository, &args);
            }

            Commands::Add(args) => {
                add_to_sum(&repository, &args);
                // load the definition
                // allocate new numbers
                // create the symbolic refs
            }
            Commands::Remove(args) => {
                remove_from_sum(&repository, &args);
            }
        }
    } else if let Some(args) = clip.define_or_show_args {
        if args.len() == 1 {
            let args = ShowArgs{name: args[0].clone()};
            describe_sum(&repository, &args);
        } else {
            define_sum(&repository,
                       &args[0],
                       &args[1..],
                       None);
            // .expect("should not attempt to recreate existing sum");
        }
    } else {
        list_sums(&repository);
    }
}

fn list_sums(repository: &Repository) {
    let ref_iterator = sums(repository);

    for r in ref_iterator {
        println!("{}", r);
    }
}

fn describe_sum(repository: &Repository, args: &ShowArgs) {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name).unwrap();
    if let GitHierarchy::Sum(sum) = gh {
        //        sum: &git_hierarchy::git_hierarchy::Sum<'repo>

        println!("sum {}", sum_fmt(sum.name()));
        let summands = sum.summands(repository);
        for s in &summands {
            println!("\t {}", s.name().unwrap());
        }
        // report if clean or dirty.

        // prune non-existings summands ??? why?
        // fn show_prune_definition(){unimplemented!()}
    }
}

fn add_to_sum(repository: &Repository, args: &AddArgs) {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name).unwrap();
    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs : Vec<Reference>
            = args.summands.iter().map(|x| {
                repository.resolve_reference_from_short_name(x.as_ref()).unwrap()
            }).collect();

        sum.add_summands(repository, sumrefs.iter(), None).expect("failed to add summands");
    }
}

fn remove_from_sum(repository: &Repository, args: &RemoveArgs) {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name).unwrap();

    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs = resolve_references_from_user(repository, &args.summands);
        sum.remove_summands(repository, sumrefs.iter()).expect("failed to add summands");
    }

}


/*
fn git_sum_branches() {unimplemented!()}


fn remove_from_sum() {unimplemented!()}
*/

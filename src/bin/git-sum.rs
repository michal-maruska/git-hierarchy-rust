use std::path::PathBuf;
use std::process::exit;
use clap::{Parser,Subcommand};
use git2::{Repository,Reference,Oid};

#[allow(unused_imports)]
use git_hierarchy::git_hierarchy::{GitHierarchy,Segment,Sum,load,sums, sum_fmt};
use git_hierarchy::rebase::check_summands;


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
    /// List all sums
    #[command(name="list", long_flag("list"),short_flag('l'))]
    List(ListArgs),

    /// delete the sum itself.
    #[command(name="delete", long_about = "yes, ", short_flag('d'))]
    Delete(DeleteCmd),

    /// define a new sum
    #[command(name="define", long_about = None,long_flag("define"),short_flag('D'))]
    Define(DefineArgs),

    /// dumnp the definition of the sum
    #[command(name="show", long_about = None,long_flag("show"),short_flag('s'))]
    Show(ShowArgs),

    /// add additional summands
    #[command(name="add", long_about = None,long_flag("add"),short_flag('a'))]
    Add(AddArgs),

    /// remove the summand
    #[command(name="remove", long_about = None,long_flag("remove"),short_flag('r'))]
    Remove(RemoveArgs),
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
                                            hint: Option<T>) -> Result<(), git2::Error> {
    let sumrefs = resolve_references_from_user(repository, summands)?;

    let mut hint_head_oid = None;

    if let Some(s) = hint {
        if let Ok(sha) = Oid::from_str(s.as_ref()) {
            if let Ok(commit) = repository.find_commit(sha) {
                hint_head_oid = Some(commit);
            } else {
                debug!("couldn't resolve {}", sha)
            }
        } else {
            debug!("not a valid commit id {}", s.as_ref());
        }
    }

    Sum::create(
        repository,
        name,
        sumrefs.iter(),
        hint_head_oid
    )?;
    Ok(())
}

fn delete_sum(repository: &Repository, args: &DeleteCmd) -> Result<(), git2::Error> {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.sum_name)?;
    if let GitHierarchy::Sum(sum) = gh {
        info!("deleting {}", args.sum_name);
        sum.reference.borrow_mut().delete()?;
        let mut first_err = None;
        for mut summand in sum.summands {
            if let Err(e) = summand.delete() {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        if let Some(e) = first_err {
            return Err(e);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                           args.head)?;
            }
            Commands::Delete(args) => {
                delete_sum(&repository, &args)?;
            }
            Commands::Show(args) => {
                describe_sum(&repository, &args)?;
            }

            Commands::Add(args) => {
                add_to_sum(&repository, &args)?;
            }
            Commands::Remove(args) => {
                remove_from_sum(&repository, &args)?;
            }
        }
    } else if let Some(args) = clip.define_or_show_args {
        if args.len() == 1 {
            let args = ShowArgs{name: args[0].clone()};
            describe_sum(&repository, &args)?;
        } else {
            define_sum(&repository,
                       &args[0],
                       &args[1..],
                       None)?;
        }
    } else {
        list_sums(&repository);
    }
    Ok(())
}

fn list_sums(repository: &Repository) {
    match sums(repository) {
        Ok(ref_iterator) => {
            for r in ref_iterator {
                println!("{}", r);
            }
        }
        Err(e) => {
            eprintln!("failed to list sums: {}", e);
            exit(1);
        }
    }
}

fn describe_sum(repository: &Repository, args: &ShowArgs) -> Result<(), git2::Error> {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name)?;
    if let GitHierarchy::Sum(sum) = gh {
        println!("sum {}", sum_fmt(sum.name()));
        let summands = sum.summands(repository);
        for s in &summands {
            println!("\t {}", s.name().unwrap());
        }
        let summands_gh: Result<Vec<GitHierarchy<'_>>, _> =
            summands.into_iter().map(|x|
                git_hierarchy::git_hierarchy::load(repository, x.name().unwrap())).collect();
        let summands_gh = summands_gh?;

        let summands_refs = summands_gh.iter().collect();

        if let Err(_e) = check_summands(repository, &sum, &sum.parent_commits(), &summands_refs) {
            eprint!("Sum is not up-to-date");
        }
    }
    Ok(())
}

fn resolve_references_from_user<'repo, S, VS>(
    repository: &'repo Repository,
    names: VS,
) -> Result<Vec<Reference<'repo>>, git2::Error>
where
    VS: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut refs = Vec::new();
    for x in names {
        let name = x.as_ref();
        if !Segment::name_is_valid(name)? {
            return Err(git2::Error::from_str(&format!(
                "invalid reference name: {}",
                name
            )));
        }
        let r = repository.resolve_reference_from_short_name(name)?;
        refs.push(r);
    }
    Ok(refs)
}

fn add_to_sum(repository: &Repository, args: &AddArgs) -> Result<(), git2::Error> {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name)?;
    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs = resolve_references_from_user(repository, &args.summands)?;
        sum.add_summands(repository, sumrefs.iter(), None)?;
    }
    Ok(())
}

fn remove_from_sum(repository: &Repository, args: &RemoveArgs) -> Result<(), git2::Error> {
    let gh = git_hierarchy::git_hierarchy::load(repository, &args.name)?;

    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs = resolve_references_from_user(repository, &args.summands)?;
        sum.remove_summands(repository, sumrefs.iter())?;
    }
    Ok(())
}


/*
fn git_sum_branches() {unimplemented!()}


fn remove_from_sum() {unimplemented!()}
*/

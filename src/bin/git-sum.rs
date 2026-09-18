use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser,Subcommand};
use git2::Repository;
use colored::Colorize;

use git_hierarchy::cli::{ClapGitRepo, resolve_references_from_user};
use git_hierarchy::git_hierarchy::{GitHierarchy, Sum, load, sums, sum_fmt};
use git_hierarchy::rebase::check_summands;
use git_hierarchy::base::resolve_to_commit_maybe;

#[allow(unused_imports)]
use tracing::{debug,info,error};

/// Manage Sum information -- merge definitions
#[derive(Parser)]
#[command(version, long_about = None)] // how to use the comment above?
#[command(subcommand_negates_reqs = true)]
// I need -g to be usable with sumcommands:
// #[command(args_conflicts_with_subcommands = true)] // positional arguments
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


fn define_sum<'repo, 'a, T: AsRef<str> + 'a>(
    repository: &'repo Repository,
    name: &str,
    summands: &[T],
    hint: Option<T>,
) -> Result<()> {
    let sumrefs = resolve_references_from_user(repository, summands)
        .with_context(|| format!("failed to resolve summand references for sum '{}'", name))?;
    let hint_head_oid = resolve_to_commit_maybe(repository, hint)
        .context("failed to resolve hint commit for sum")?;

    let _sum = Sum::create(repository, name, sumrefs.iter(), hint_head_oid)
        .with_context(|| format!("failed to create sum '{}'", name))?;
    Ok(())
}

fn delete_sum(repository: &Repository, args: &DeleteCmd) -> Result<()> {
    let gh = load(repository, &args.sum_name)?;
    if let GitHierarchy::Sum(sum) = gh {
        info!("deleting {}", args.sum_name);
        sum.reference.borrow_mut().delete().context("failed to delete sum reference")?;

        let mut first_err = None;
        for mut summand in sum.summands {
            if let Err(e) = summand.delete() {
                eprintln!("{}: {}", Colorize::red("failed to drop summand reference"), e);
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        if let Some(e) = first_err {
            return Err(e).context("failed to delete all summand references");
        }
    } else {
        bail!("{} is not a sum", args.sum_name);
    }
    Ok(())
}

fn main() -> Result<()> {
    let clip = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(clip.verbosity)
        .init();

    let repository = clip.git_repository.open()?;

    if let Some(command) = clip.command {
        match command {
            Commands::List(_args) => {
                list_sums(&repository)?;
            }
            Commands::Define(args) => {
                define_sum(&repository, &args.name, &args.components, args.head)?;
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
            let args = ShowArgs { name: args[0].clone() };
            describe_sum(&repository, &args)?;
        } else {
            define_sum(&repository, &args[0], &args[1..], None)?;
        }
    } else {
        list_sums(&repository)?;
    }
    Ok(())
}

fn list_sums(repository: &Repository) -> Result<()> {
    let ref_iterator = sums(repository).context("failed to query sums")?;
    for r in ref_iterator {
        println!("{}", r);
    }
    Ok(())
}

fn describe_sum(repository: &Repository, args: &ShowArgs) -> Result<()> {
    let gh = load(repository, &args.name)?;
    if let GitHierarchy::Sum(sum) = gh {
        println!("sum {}", sum_fmt(sum.name()));
        let summands = sum.summands(repository);
        for s in &summands {
            println!("\t {}", s.name().unwrap_or(""));
        }

        let summands_gh: Result<Vec<GitHierarchy<'_>>, _> =
            summands.into_iter().map(|x| {
                let name = x.name().ok_or_else(|| anyhow!("summand reference missing name"))?;
                load(repository, name)
            }).collect();
        let summands_gh = summands_gh?;

        let summands_refs = summands_gh.iter().collect();

        if let Err(_e) = check_summands(repository, &sum, &sum.parent_commits(), &summands_refs) {
            eprint!("Sum is not up-to-date");
        }
        Ok(())
    } else {
        bail!("{} is not a sum", args.name);
    }
}


fn add_to_sum(repository: &Repository, args: &AddArgs) -> Result<()> {
    let gh = load(repository, &args.name)?;
    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs = resolve_references_from_user(repository, &args.summands)
            .with_context(|| format!("failed to resolve summands for sum '{}'", args.name))?;
        sum.add_summands(repository, sumrefs.iter(), None)
            .with_context(|| format!("failed to add summands to sum '{}'", args.name))?;
        Ok(())
    } else {
        bail!("{} is not a sum", args.name);
    }
}

fn remove_from_sum(repository: &Repository, args: &RemoveArgs) -> Result<()> {
    let gh = load(repository, &args.name)?;

    if let GitHierarchy::Sum(mut sum) = gh {
        let sumrefs = resolve_references_from_user(repository, &args.summands)
            .with_context(|| format!("failed to resolve summands for sum '{}'", args.name))?;
        sum.remove_summands(repository, sumrefs.iter())
            .with_context(|| format!("failed to remove summands from sum '{}'", args.name))?;
        Ok(())
    } else {
        bail!("{} is not a sum", args.name);
    }
}



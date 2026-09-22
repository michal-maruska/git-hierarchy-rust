use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser,Subcommand,CommandFactory,FromArgMatches};
use git_hierarchy::base::{is_linear_ancestor,resolve_user_commit};
use git_hierarchy::cli::ClapGitRepo;
use git2::{Repository, build::CheckoutBuilder};

use git_hierarchy::git_hierarchy::{GitHierarchy,Segment,segments, load, segment_fmt};

#[allow(unused_imports)]
use tracing::{debug,info};

/// Operate on segments or 1 segment
#[derive(Parser)] // Debug
// about ... Description from Cargo.toml
#[command(version, long_about = None)]
#[command(subcommand_negates_reqs = true)]
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


fn define<'repo>(repository: &'repo Repository, args: &DefineArgs) -> Result<Segment<'repo>> {
    if !Segment::name_is_valid(&args.segment_name)? || !Segment::name_is_valid(&args.base)? {
        bail!("invalid reference name");
    }
    let base = repository.resolve_reference_from_short_name(&args.base)
        .with_context(|| format!("failed to resolve base reference '{}'", args.base))?;

    let start = if let Some(s) = &args.start {
        resolve_user_commit(repository, s)
            .with_context(|| anyhow!("failed to resolve start commit '{}'", s))?
            .id()
    } else {
        base.target().ok_or_else(|| anyhow!("base reference target missing"))?
    };

    let head = if let Some(x) = &args.head {
        resolve_user_commit(repository, x)
            .with_context(|| anyhow!("failed to resolve head commit '{}'", x))?
            .id()
    } else {
        start
    };

    info!("create {} in {:?}", args.segment_name, repository.path());
    info!("base = {}, start {} = {}", base.name().unwrap_or("??"), start, head);
    Segment::create(repository, &args.segment_name, &base, start, head)
        .with_context(|| format!("failed to define segment '{}'", args.segment_name))
}

fn delete(repository: &Repository, args: &DeleteCmd) -> Result<()> {
    let gh = load(repository, &args.segment_name)?;
    if let GitHierarchy::Segment(mut segment) = gh {
        println!("Delete {} in {:?}", args.segment_name, repository.path());

        segment.base.borrow_mut().delete().context("failed to delete segment base reference")?;
        segment._start.delete().context("failed to delete segment start reference")?;
        segment.reference.borrow_mut().delete().context("failed to delete segment reference")?;
        Ok(())
    } else {
        bail!("{} is not a segment", args.segment_name);
    }
}

// see list_segment in git-walk-down.rs
fn describe(repository: &Repository, segment_name: &str) -> Result<()> {
    let gh = load(repository, segment_name)?;

    if let GitHierarchy::Segment(segment) = gh {
        println!("Segment {} in {:?}", segment_fmt(segment_name), repository.path());

        println!("Base {}", segment.base(repository).name().unwrap_or(""));
        println!("Start {} lenght {} {}", segment.start(),
                 segment.iter(repository)?.count(),
                 if segment.uptodate(repository) { "clean" } else { "dirty"}
        );
        if !is_linear_ancestor(repository,
            segment.start(),
            segment.reference.borrow().peel_to_commit()?.id()
        )? {
            bail!("Start is not ancestor!");
        }

        for oid in segment.iter(repository)? {
            let oid = oid?;
            let commit = repository.find_commit(oid)?;
            println!("{}: {}", oid, commit.summary().unwrap_or(""));
        }
        Ok(())
    } else {
        info!("Segment {} does not exist", segment_fmt(segment_name));
        bail!("{} is not a segment", segment_name);
    }
}

fn list_segments(repository: &Repository) -> Result<()> {
    let ref_iterator = segments(repository).context("failed to query segments")?;
    for r in ref_iterator {
        println!("{}", segment_fmt(&r));
    }
    Ok(())
}


fn main() -> Result<()> {

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

    let repository = clip.git_repository.open()?;

    // this is an associated function, not a method
    if let Some(command) = clip.command {
        match command {
            Commands::List(_args) => {
                list_segments(&repository)?;
            }
            Commands::Restart(args) => {
                let gh = load(&repository, &args.segment_name)?;
                if let GitHierarchy::Segment(segment) = gh {
                    let commit = resolve_user_commit(&repository, args.commit.as_ref())
                        .with_context(|| anyhow!("failed to resolve commit '{}'", args.commit))?;
                    let oid = commit.id();
                    println!("restart from {} {}", args.commit, oid);
                    segment.set_start(&repository, oid, "restart")
                        .with_context(|| format!("failed to set start of segment '{}' to {}", args.segment_name, oid))?;
                } else {
                    bail!("{} is not a segment", args.segment_name);
                }

            },
            Commands::Update(args) => {
                Segment::check_name_is_valid(&args.new_base)?;
                let gh = load(&repository, &args.segment_name)?;
                if let GitHierarchy::Segment(segment) = gh {
                    let new_base = repository.resolve_reference_from_short_name(&args.new_base)
                        .with_context(|| format!("failed to resolve reference '{}'", args.new_base))?;
                    println!("rebase from {} -> {} {}", args.new_base,
                             new_base.name().unwrap_or(""),
                             if args.rebase {"immediately"} else {""});
                    segment.set_base(&repository, &new_base);
                } else {
                    bail!("{} is not a segment", args.segment_name);
                }
            },
            Commands::Delete(args) => {
                delete(&repository, &args)?;
            },
            Commands::Create(args) => {
                // checkout immediate
                let seg = define(&repository, &args)?;

                // try to switch
                // let reference = seg.reference_clone(repository);
                let name = seg.reference.borrow().name().ok_or_else(|| anyhow!("reference missing name"))?.to_string();
                let reference = repository.find_reference(&name)
                    .with_context(|| format!("failed to find reference '{}'", name))?;
                println!("should checkout now {}", name);

                // 1. Checkout the TARGET tree first (while HEAD still points at the old ref)
                let target_commit = reference.peel_to_commit()
                    .with_context(|| format!("failed to peel reference '{}' to commit", name))?;
                let target_tree = target_commit.tree()
                    .with_context(|| format!("failed to get tree for commit '{}'", target_commit.id()))?;
                repository.checkout_tree(target_tree.as_object(), Some(CheckoutBuilder::new().safe()))
                    .with_context(|| format!("failed to checkout tree for '{}'", name))?;

                // 2. THEN move HEAD to point at the new ref
                repository.set_head(&name)
                    .with_context(|| format!("failed to set HEAD to '{}'", name))?;
            }
            Commands::Define(args) => {
                define(&repository, &args)?;
            },
        }
    } else if let Some(args) = clip.define_or_show_args {
        if args.is_empty() {
            unreachable!("cannot be Some, and empty vector");
        } else if args.len() == 1 {
            describe(&repository, &args[0])?;
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
            define(&repository, &def)?;
        }
    } else {
        list_segments(&repository)?;
    }
    // else nothing. Or list?
    // return Err(error.into());
    Ok(())
}

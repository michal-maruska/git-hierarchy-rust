#![deny(elided_lifetimes_in_paths)]

use clap::Parser;

use std::process::exit;
use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::git_hierarchy::{GitHierarchy, Segment};
use git_hierarchy::rebase::{check_segment, rebase_segment};
use git_hierarchy::utils::{init_tracing};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    // todo: continue -> use git-rebase-poset -c
    // should this be an invocation of git-rebase-poset?
    segment_name: String,
}

// should we check the segment first?
fn main() -> Result<(), Box<dyn std::error::Error>>{
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let repository = cli.git_repository.open()?;

    if !Segment::name_is_valid(&cli.segment_name)? {
        eprintln!("invalid segment name: {}", cli.segment_name);
        exit(1);
    }

    // continue...
    let gh = git_hierarchy::git_hierarchy::load(&repository, &cli.segment_name)?;
    if let GitHierarchy::Segment(segment) = gh {
        check_segment(&repository, &segment)?;
        rebase_segment(&repository, &segment)?;
    } else {
        eprintln!("{} is not a segment", cli.segment_name);
        exit(1);
    }
    Ok(())
}

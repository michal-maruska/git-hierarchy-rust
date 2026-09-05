#![deny(elided_lifetimes_in_paths)]

use std::path::PathBuf;
use clap::Parser;

use std::process::exit;
use git_hierarchy::git_hierarchy::{GitHierarchy, Segment};
use git_hierarchy::rebase::{check_segment, rebase_segment};
use git_hierarchy::utils::{init_tracing};
use git_hierarchy::base::open_repository;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, short = 'g')]
    directory: Option<PathBuf>,

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

    let repository = match open_repository(cli.directory.as_ref()) {
        Ok(repository) => repository,
        Err(e) => {
            eprintln!("failed to open repository: {}", e);
            exit(1);
        }
    };

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

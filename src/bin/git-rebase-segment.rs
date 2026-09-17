#![deny(elided_lifetimes_in_paths)]

use anyhow::{Context, Result, bail};
use clap::Parser;

use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::git_hierarchy::{GitHierarchy, Segment};
use git_hierarchy::rebase::{check_segment, rebase_segment};
use git_hierarchy::utils::init_tracing;

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let repository = cli.git_repository.open().context("failed to open git repository")?;

    if !Segment::name_is_valid(&cli.segment_name)? {
        bail!("invalid segment name: {}", cli.segment_name);
    }

    let gh = git_hierarchy::git_hierarchy::load(&repository, &cli.segment_name)
        .with_context(|| format!("failed to load segment '{}'", cli.segment_name))?;
    if let GitHierarchy::Segment(segment) = gh {
        check_segment(&repository, &segment)
            .with_context(|| format!("check failed for segment '{}'", cli.segment_name))?;
        rebase_segment(&repository, &segment)
            .with_context(|| format!("failed to rebase segment '{}'", cli.segment_name))?;
    } else {
        bail!("{} is not a segment", cli.segment_name);
    }
    Ok(())
}

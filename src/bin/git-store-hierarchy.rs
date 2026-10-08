use anyhow::{Context, Result};
use clap::Parser;
use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::hierarchy_store::HierarchyData;
use std::path::PathBuf;

/// Store git hierarchy metadata to a branch or file
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[command(flatten)]
    git_repository: ClapGitRepo,

    /// Branch to store hierarchy metadata to
    #[arg(short, long, default_value = "_history")]
    branch: String,

    /// File to write hierarchy metadata to
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Print serialized hierarchy metadata to stdout
    #[arg(long)]
    stdout: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(cli.verbosity)
        .init();

    let repository = cli.git_repository.open()?;
    let hierarchy = HierarchyData::collect(&repository)
        .context("failed to collect hierarchy data")?;

    if cli.stdout {
        print!("{}", hierarchy.serialize());
    } else if let Some(file_path) = cli.file {
        hierarchy.store_to_file(&file_path)
            .with_context(|| format!("failed to store hierarchy to file {:?}", file_path))?;
        println!("Stored git hierarchy metadata to file {:?}", file_path);
    } else {
        let commit_oid = hierarchy.store_to_branch(&repository, &cli.branch)
            .with_context(|| format!("failed to store hierarchy to branch '{}'", cli.branch))?;
        println!("Stored git hierarchy metadata to branch '{}' ({})", cli.branch, commit_oid);
    }

    Ok(())
}

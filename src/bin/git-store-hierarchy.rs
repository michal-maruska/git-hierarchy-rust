use anyhow::{Context, Result, bail};
use clap::Parser;
use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::hierarchy_store::HierarchyData;
use std::path::PathBuf;

/// Store git hierarchy metadata to a tag, branch or file
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[command(flatten)]
    git_repository: ClapGitRepo,

    /// Annotated tag to store hierarchy metadata into (its message
    /// carries the serialization)
    #[arg(short, long, requires_all = ["commit", "message"])]
    tag: Option<String>,

    /// Commit the tag points at (any revspec)
    #[arg(short, long)]
    commit: Option<String>,

    /// Message prefix of the tag (the serialization is appended)
    #[arg(short, long)]
    message: Option<String>,

    /// Root of the subtree to store: only its descendants are kept
    root: Option<String>,

    /// Branch to store hierarchy metadata to
    #[arg(short = 'b', long, default_value = "_history")]
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

    if cli.tag.is_none() && (cli.commit.is_some() || cli.message.is_some()) {
        bail!("-c/-m only make sense together with -t");
    }

    let repository = cli.git_repository.open()?;
    let hierarchy = if let Some(root) = &cli.root {
        // follow the example of git-walk-down: discovery DFS from the
        // root (its git-ancestors); a broken graph still yields the
        // reachable records -- unlike the whole-namespace sweep.
        HierarchyData::collect_below(&repository, root)
            .with_context(|| format!("failed to discover the hierarchy below '{}'", root))?
    } else {
        HierarchyData::collect(&repository)
            .context("failed to collect hierarchy data")?
    };

    if cli.stdout {
        print!("{}", hierarchy.serialize());
    } else if let Some(file_path) = cli.file {
        hierarchy.store_to_file(&file_path)
            .with_context(|| format!("failed to store hierarchy to file {:?}", file_path))?;
        println!("Stored git hierarchy metadata to file {:?}", file_path);
    } else if let Some(tag_name) = &cli.tag {
        let commit_ish = cli.commit.as_deref().unwrap_or("HEAD");
        let commit = repository
            .revparse_single(commit_ish)
            .with_context(|| format!("failed to resolve commit '{}'", commit_ish))?
            .peel_to_commit()
            .with_context(|| format!("'{}' does not peel to a commit", commit_ish))?;
        let tag_oid = hierarchy
            .store_to_tag(&repository, tag_name, &commit, cli.message.as_deref().unwrap())
            .with_context(|| format!("failed to store hierarchy to tag '{}'", tag_name))?;
        println!("Stored git hierarchy metadata to tag '{}' ({})", tag_name, tag_oid);
    } else {
        let commit_oid = hierarchy.store_to_branch(&repository, &cli.branch)
            .with_context(|| format!("failed to store hierarchy to branch '{}'", cli.branch))?;
        println!("Stored git hierarchy metadata to branch '{}' ({})", cli.branch, commit_oid);
    }

    Ok(())
}

use anyhow::{Context, Result, bail};
use clap::Parser;
use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::hierarchy_store::HierarchyData;
use std::io::{self, Read};
use std::path::PathBuf;

/// Restore git hierarchy metadata from a tag, branch, file, or stdin
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[command(flatten)]
    git_repository: ClapGitRepo,

    /// Tag whose message carries the serialized section (the mirror
    /// of what git-store-hierarchy stores)
    #[arg(short, long)]
    tag: Option<String>,

    /// Dump the extracted serialized section to stdout, don't restore
    #[arg(long)]
    stdout: bool,

    /// Branch to load hierarchy metadata from
    #[arg(short, long, default_value = "_history")]
    branch: String,

    /// File to load hierarchy metadata from
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Read serialized hierarchy metadata from stdin
    #[arg(long)]
    stdin: bool,

    /// Dry run: show restoration plan without updating references
    #[arg(short = 'n', long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(cli.verbosity)
        .init();

    if cli.stdout && cli.tag.is_none() {
        bail!("--stdout only makes sense together with --tag");
    }

    let repository = cli.git_repository.open()?;

    if cli.stdout {
        let tag_name = cli.tag.as_deref().unwrap();
        let section = HierarchyData::read_tag_section(&repository, tag_name)
            .with_context(|| format!("failed to extract the hierarchy section from tag '{}'", tag_name))?;
        print!("{}", section);
        return Ok(());
    }

    let hierarchy = if let Some(tag_name) = &cli.tag {
        HierarchyData::load_from_tag(&repository, tag_name)
            .with_context(|| format!("failed to load hierarchy data from tag '{}'", tag_name))?
    } else if cli.stdin {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)
            .context("failed to read hierarchy data from stdin")?;
        HierarchyData::deserialize(&buffer)
            .context("failed to parse hierarchy data from stdin")?
    } else if let Some(file_path) = cli.file {
        HierarchyData::load_from_file(&file_path)
            .with_context(|| format!("failed to load hierarchy data from file {:?}", file_path))?
    } else {
        HierarchyData::load_from_branch(&repository, &cli.branch)
            .with_context(|| format!("failed to load hierarchy data from branch '{}'", cli.branch))?
    };

    let report = hierarchy.restore(&repository, cli.dry_run)?;
    print!("{}", report);

    if cli.dry_run {
        println!("\nDry run mode: No references were updated.");
    } else {
        println!("\nHierarchy restoration complete.");
    }

    Ok(())
}

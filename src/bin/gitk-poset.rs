use clap::Parser;

use std::process::{Command, exit};

use git_hierarchy::cli::ClapGitRepo;
use git_hierarchy::git_hierarchy::{GitHierarchy, Segment};
use git_hierarchy::graph::discover_pet::find_hierarchy;
use git_hierarchy::rebase::check_sum;
use git_hierarchy::base::current_branch;
use git_hierarchy::utils::init_tracing;
use tracing::info;

#[derive(Parser, Debug)]
#[command(version, verbatim_doc_comment, about = "Invoke gitk on tops and bases of a poset hierarchy")]
struct Cli {
    #[command(flatten)]
    git_repository: ClapGitRepo,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Print command line without executing gitk
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    root_reference: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let repository = cli.git_repository.open()?;

    let root = match cli.root_reference {
        Some(r) => r,
        None => {
            let head = current_branch(&repository)
                .ok_or_else(|| git2::Error::from_str("no current branch chosen"))?;
            info!("Start from HEAD = {}", head);
            head
        }
    };
    Segment::check_name_is_valid(&root)?;

    let hierarchy_graph = find_hierarchy(&repository, root.clone());

    let mut tops = Vec::new();
    tops.push(root.clone());

    let mut bases = Vec::new();

    for v in &hierarchy_graph.discovery_order {
        if let Some(node) = hierarchy_graph.labeled_objects.get(v) {
            match node {
                GitHierarchy::Segment(segment) => {
                    if !segment.uptodate(&repository) {

                        // the base is off.
                        let name = segment.base(&repository).name().unwrap().to_string();
                        if !tops.contains(&name) {
                            tops.push(name);
                        }
                    }
                }
                GitHierarchy::Sum(sum) => {
                    if check_sum(&repository, sum, &hierarchy_graph.labeled_objects).is_err() {
                        // todo: add the summands which are off.
                        let name = sum.name().to_string();
                        if !tops.contains(&name) {
                            tops.push(name);
                        }
                    }
                }
                GitHierarchy::Reference(r) => {
                    let ref_name = r.name().unwrap_or(v);
                    let base_arg = format!("^{}", ref_name);
                    if !bases.contains(&base_arg) {
                        bases.push(base_arg);
                    }
                }
                GitHierarchy::Name(_) => {}
            }
        }
    }

    let mut gitk_args = tops;
    gitk_args.extend(bases);

    if cli.dry_run {
        println!("gitk {}", gitk_args.join(" "));
        return Ok(());
    }

    let workdir = repository.workdir().unwrap_or_else(|| repository.path());

    let mut cmd = Command::new("gitk");
    cmd.args(&gitk_args);
    cmd.current_dir(workdir);

    info!("Executing: gitk {}", gitk_args.join(" "));

    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                if let Some(code) = status.code() {
                    exit(code);
                } else {
                    exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to execute gitk: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

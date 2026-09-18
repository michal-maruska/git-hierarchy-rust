use anyhow::{Context, Result};
use std::path::PathBuf;
use git2::Repository;
use crate::base::open_repository;
use crate::git_hierarchy::Segment;

/// Common command-line argument for specifying target Git repository directory.
#[derive(clap::Args, Debug, Clone)]
#[command(name = "git", about = None, long_about = None)]
pub struct ClapGitRepo {
    #[arg(long, short = 'g')]
    #[arg(global = true)]
    pub directory: Option<PathBuf>,
}

impl ClapGitRepo {
    pub fn open(&self) -> Result<Repository> {
        open_repository(self.directory.as_ref()).context("failed to open git repository")
    }
}

/// Resolves a collection of reference names (provided as short names or full reference names)
/// into `git2::Reference` instances after validating that each name is valid.
pub fn resolve_references_from_user<'repo, S, VS>(
    repository: &'repo Repository,
    names: VS,
) -> Result<Vec<git2::Reference<'repo>>>
where
    VS: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut refs = Vec::new();
    for x in names {
        let name = x.as_ref();
        Segment::check_name_is_valid(name)?;
        let r = repository
            .resolve_reference_from_short_name(name)
            .with_context(|| format!("failed to resolve reference '{}'", name))?;
        refs.push(r);
    }
    Ok(refs)
}

/// Resolves a slice of reference name strings in-place to their full reference names (if found),
/// after validating each string with `Segment::check_name_is_valid`.
pub fn resolve_reference_names_from_user(
    repository: &Repository,
    names: &mut [String],
) -> Result<()> {
    for e in names {
        Segment::check_name_is_valid(e)?;
        if let Ok(r) = repository.resolve_reference_from_short_name(e) {
            if let Some(n) = r.name() {
                *e = n.to_string();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestRepo;

    #[test]
    fn test_resolve_references_rejects_invalid_names() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let invalid_names = vec!["-option-inject", "--flag"];
        let res = resolve_references_from_user(repo, invalid_names);
        match res {
            Err(e) => assert!(e.to_string().contains("invalid reference name")),
            Ok(_) => panic!("expected error for invalid reference name"),
        }
    }

    #[test]
    fn test_resolve_reference_names_from_user() {
        let test_repo = TestRepo::new();
        let repo = &test_repo.repo;

        let mut names = vec!["master".to_string(), "-invalid".to_string()];
        let res = resolve_reference_names_from_user(repo, &mut names);
        assert!(res.is_err());
    }
}

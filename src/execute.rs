use git2::Repository;
pub use std::process::{Command, ExitStatus};

#[allow(unused)]
use tracing::{debug, info, warn, error};

#[derive(Debug)]
pub enum Error {
    NoWorkDir,
    ProcessError(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoWorkDir => write!(f, "Repository has no working directory"),
            Error::ProcessError(e) => write!(f, "Process execution failed: {}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::ProcessError(e) => Some(e),
            _ => None,
        }
    }
}

/// Invoke git with the given CLI arguments. In the directory of the @repository.
pub fn git_run(repository: &Repository, cmd_line: &[&str]) -> Result<ExitStatus, Error> {
    let mut command = Command::new("git");
    command.args(cmd_line);

    let workdir = repository.workdir().ok_or(Error::NoWorkDir)?;
    command.current_dir(workdir);
    debug!("must cd into {}", workdir.display());
    warn!("git-run: {}", cmd_line.join(" "));

    let child = command.spawn().map_err(Error::ProcessError)?;
    let output = child.wait_with_output().map_err(Error::ProcessError)?;

    debug!("git command status: {}", output.status);
    Ok(output.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestRepo;
    use std::fs;

    #[test]
    fn test_git_run_success() {
        let test_repo = TestRepo::new();
        let status = git_run(&test_repo.repo, &["status"]).unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_git_run_no_workdir() {
        let path = std::env::temp_dir().join(format!("git_hierarchy_bare_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        let repo = Repository::init_bare(&path).unwrap();

        let result = git_run(&repo, &["status"]);
        assert!(matches!(result, Err(Error::NoWorkDir)));

        let _ = fs::remove_dir_all(&path);
    }
}

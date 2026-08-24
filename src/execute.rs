use git2::Repository;
pub use std::process::{Command, ExitStatus};

#[allow(unused)]
use tracing::{debug, info, warn, error};


pub enum Error {
    NoWorkDir,
    ProcessError(std::io::Error),
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

    dbg!(output.status);
    Ok(output.status)
}

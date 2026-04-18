//! Git helpers — shells out to the `git` CLI (matches `net.git-fetch-with-cli`).

use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git command failed: {0}")]
    Command(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Clone `url` into `dest` if empty; otherwise `git fetch` in place.
pub fn clone_or_fetch(url: &str, dest: &Path) -> Result<(), GitError> {
    if dest.exists() && dest.join(".git").exists() {
        run_git(dest, ["fetch", "--tags", "--prune", "--force"])?;
        Ok(())
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        run_git(
            &PathBuf::from("."),
            [
                "clone",
                "--quiet",
                "--no-checkout",
                url,
                &dest.to_string_lossy(),
            ],
        )?;
        Ok(())
    }
}

/// Check out a specific ref in `repo` — accepts tag / branch / sha.
pub fn checkout(repo: &Path, gitref: &str) -> Result<(), GitError> {
    run_git(repo, ["checkout", "--quiet", "--detach", gitref])
}

/// Return the current HEAD SHA of `repo`.
pub fn head_sha(repo: &Path) -> Result<String, GitError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(GitError::Command(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run_git<'a, A>(cwd: &Path, args: A) -> Result<(), GitError>
where
    A: IntoIterator<Item = &'a str>,
{
    let out = Command::new("git").current_dir(cwd).args(args).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::Command(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }
}

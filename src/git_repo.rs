//! Git repository detection and path resolution.
//!
//! This module provides functionality to locate git repositories by walking
//! up the directory tree and handles both regular repositories and git worktrees
//! (where `.git` is a file pointing to the actual git directory).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// Finds the nearest git repository by walking parents looking for `.git`.
/// Returns (repo_root, git_dir_path).
pub fn find_git_repo(start: &Path) -> Result<Option<(PathBuf, PathBuf)>> {
    let mut current = start.to_path_buf();

    loop {
        let dot_git = current.join(".git");
        if dot_git.is_dir() {
            return Ok(Some((current, dot_git)));
        }

        if dot_git.is_file() {
            let git_dir = parse_gitdir_file(&dot_git)?;
            return Ok(Some((current, git_dir)));
        }

        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent.to_path_buf();
    }
}

fn parse_gitdir_file(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = fs::read_to_string(dot_git_file).with_context(|| {
        format!(
            "Failed to read .git file at {} (worktree?)",
            dot_git_file.display()
        )
    })?;

    let trimmed = contents.trim();
    let prefix = "gitdir:";
    let Some(rest) = trimmed.strip_prefix(prefix) else {
        return Err(anyhow!(
            "Unsupported .git file format at {}",
            dot_git_file.display()
        ));
    };

    let gitdir_raw = rest.trim();
    if gitdir_raw.is_empty() {
        return Err(anyhow!(
            "Invalid gitdir in .git file at {}",
            dot_git_file.display()
        ));
    }

    let gitdir_path = PathBuf::from(gitdir_raw);
    if gitdir_path.is_absolute() {
        return Ok(gitdir_path);
    }

    let parent = dot_git_file
        .parent()
        .ok_or_else(|| anyhow!("Invalid .git path: {}", dot_git_file.display()))?;

    Ok(parent.join(gitdir_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_gitdir_file_supports_relative_gitdir() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root)?;
        let dot_git_file = repo_root.join(".git");
        fs::write(&dot_git_file, "gitdir: .git/worktrees/foo\n")?;

        // act
        let gitdir = parse_gitdir_file(&dot_git_file)?;

        // assert
        assert_eq!(gitdir, repo_root.join(".git/worktrees/foo"));
        Ok(())
    }
}

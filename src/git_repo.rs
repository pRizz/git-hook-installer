//! Git repository detection and path resolution.
//!
//! This module provides functionality to locate git repositories by walking
//! up the directory tree and handles both regular repositories and git worktrees
//! (where `.git` is a file pointing to the actual git directory).

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// Finds the nearest git repository by walking parents looking for `.git`.
/// Returns (repo_root, git_dir_path).
pub fn find_git_repo(start: &Path) -> Result<Option<(PathBuf, PathBuf)>> {
    let mut current = start.to_path_buf();

    loop {
        if let Some(git_dir) = git_dir_from_repo_root(&current)? {
            return Ok(Some((current, git_dir)));
        }

        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent.to_path_buf();
    }
}

/// If `repo_root/.git` exists (dir or worktree file), returns the resolved git directory.
pub fn git_dir_from_repo_root(repo_root: &Path) -> Result<Option<PathBuf>> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Ok(Some(dot_git));
    }

    if dot_git.is_file() {
        let git_dir = parse_gitdir_file(&dot_git)?;
        return Ok(Some(git_dir));
    }

    Ok(None)
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

/// Finds git repositories under `scan_root`.
///
/// This is intended for "parent folder contains many repos" use-cases. To keep runtime bounded,
/// we limit the traversal depth and skip well-known large/unrelated directories.
pub fn find_git_repos_under_dir(
    scan_root: &Path,
    max_depth: usize,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    const MAX_ENTRIES: usize = 200_000;

    if !scan_root.is_dir() {
        return Err(anyhow!(
            "Scan root {} is not a directory",
            scan_root.display()
        ));
    }

    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut seen_repo_roots: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((scan_root.to_path_buf(), 0));

    let mut visited_entries: usize = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        if visited_entries >= MAX_ENTRIES {
            break;
        }
        visited_entries = visited_entries.saturating_add(1);

        if let Some(git_dir) = git_dir_from_repo_root(&dir)? {
            // If we found a repo root, don't descend into it; treat it as a terminal unit.
            if seen_repo_roots.insert(dir.clone()) {
                found.push((dir, git_dir));
            }
            continue;
        }

        if depth >= max_depth {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            if visited_entries >= MAX_ENTRIES {
                break;
            }
            visited_entries = visited_entries.saturating_add(1);

            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            // Avoid scanning huge/unrelated directories.
            if matches!(
                name,
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".venv"
                    | "__pycache__"
                    | ".tox"
                    | ".idea"
                    | ".vscode"
            ) {
                continue;
            }

            queue.push_back((path, depth + 1));
        }
    }

    found.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(found)
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

    #[test]
    fn find_git_repos_under_dir_finds_nested_repo_roots() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let root = temp.path().join("root");
        fs::create_dir_all(&root)?;

        let repo_a = root.join("repo-a");
        fs::create_dir_all(repo_a.join(".git"))?;

        let repo_b = root.join("repo-b");
        fs::create_dir_all(&repo_b)?;
        fs::write(repo_b.join(".git"), "gitdir: .gitdir/worktrees/w1\n")?;
        fs::create_dir_all(repo_b.join(".gitdir").join("worktrees").join("w1"))?;

        let not_repo = root.join("not-a-repo");
        fs::create_dir_all(&not_repo)?;

        // act
        let repos = find_git_repos_under_dir(&root, 1)?;

        // assert
        assert!(repos.iter().any(|(r, _)| r == &repo_a));
        assert!(repos.iter().any(|(r, _)| r == &repo_b));
        assert!(!repos.iter().any(|(r, _)| r == &not_repo));
        Ok(())
    }

    #[test]
    fn find_git_repos_under_dir_respects_max_depth() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let root = temp.path().join("root");
        fs::create_dir_all(&root)?;

        let nested_parent = root.join("level-1");
        let nested_repo = nested_parent.join("repo");
        fs::create_dir_all(nested_repo.join(".git"))?;

        // act
        let repos_depth_1 = find_git_repos_under_dir(&root, 1)?;
        let repos_depth_2 = find_git_repos_under_dir(&root, 2)?;

        // assert
        assert!(!repos_depth_1.iter().any(|(r, _)| r == &nested_repo));
        assert!(repos_depth_2.iter().any(|(r, _)| r == &nested_repo));
        Ok(())
    }
}

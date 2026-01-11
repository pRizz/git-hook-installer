use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use dialoguer::Select;

use crate::util::relative_display;

#[derive(Clone, Copy)]
pub struct ResolveHookOptions {
    pub yes: bool,
    pub non_interactive: bool,
}

pub fn resolve_cargo_manifest_dir(
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Result<PathBuf> {
    if let Some(manifest_dir) = maybe_manifest_dir_from_cli {
        return resolve_manifest_dir_from_cli(repo_root, manifest_dir);
    }

    let mut manifest_dirs = find_cargo_manifests_upwards(cwd, repo_root);
    if manifest_dirs.is_empty() {
        manifest_dirs = find_cargo_manifests_bfs(repo_root, 6, 8_000)?;
    }

    manifest_dirs.sort();
    manifest_dirs.dedup();

    let Some(first_dir) = manifest_dirs.first() else {
        return Err(anyhow!(
            "No Cargo.toml found in git repository at {}",
            repo_root.display()
        ));
    };

    if manifest_dirs.len() == 1 {
        return Ok(first_dir.clone());
    }

    if options.non_interactive || options.yes {
        return Err(anyhow!(
            "Multiple Cargo.toml files found; re-run with --manifest-dir to choose one"
        ));
    }

    let labels: Vec<String> = manifest_dirs
        .iter()
        .map(|dir| relative_display(repo_root, dir))
        .collect();

    let selected = Select::new()
        .with_prompt("Multiple Cargo.toml files found. Which one should the hook use?")
        .default(0)
        .items(&labels)
        .interact()
        .context("Failed to read selection from stdin")?;

    let Some(selected_dir) = manifest_dirs.get(selected) else {
        return Err(anyhow!("Invalid selection"));
    };

    Ok(selected_dir.clone())
}

fn resolve_manifest_dir_from_cli(repo_root: &Path, manifest_dir: &Path) -> Result<PathBuf> {
    let manifest_dir = normalize_path(repo_root, manifest_dir);
    ensure_is_within_repo(repo_root, &manifest_dir)?;

    let cargo_toml = manifest_dir.join("Cargo.toml");
    if cargo_toml.is_file() {
        return Ok(manifest_dir);
    }

    Err(anyhow!(
        "--manifest-dir {} does not contain a Cargo.toml",
        manifest_dir.display()
    ))
}

fn normalize_path(repo_root: &Path, input: &Path) -> PathBuf {
    if input.is_absolute() {
        return input.to_path_buf();
    }
    repo_root.join(input)
}

fn ensure_is_within_repo(repo_root: &Path, candidate: &Path) -> Result<()> {
    // We avoid canonicalize (can fail if paths don't exist). Instead, do a component-wise check.
    // This is "best effort" and assumes no symlink tricks; we still verify Cargo.toml exists.
    let repo_components: Vec<Component<'_>> = repo_root.components().collect();
    let candidate_components: Vec<Component<'_>> = candidate.components().collect();

    if candidate_components.len() < repo_components.len() {
        return Err(anyhow!(
            "Path {} is outside the repository",
            candidate.display()
        ));
    }

    for (a, b) in repo_components.iter().zip(candidate_components.iter()) {
        if a != b {
            return Err(anyhow!(
                "Path {} is outside the repository",
                candidate.display()
            ));
        }
    }

    Ok(())
}

pub fn find_cargo_manifests_upwards(cwd: &Path, repo_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut current = cwd.to_path_buf();

    loop {
        if current.join("Cargo.toml").is_file() {
            dirs.push(current.clone());
        }

        if current == repo_root {
            break;
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }

    dirs
}

fn find_cargo_manifests_bfs(
    repo_root: &Path,
    max_depth: usize,
    max_entries: usize,
) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((repo_root.to_path_buf(), 0));

    let mut visited_entries: usize = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        if visited_entries >= max_entries {
            break;
        }
        visited_entries += 1;

        if dir.join("Cargo.toml").is_file() {
            found.push(dir.clone());
        }

        if depth >= max_depth {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
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
            let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if name == ".git" || name == "target" || name == "node_modules" {
                continue;
            }

            queue.push_back((path, depth + 1));
        }
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn find_cargo_manifests_upwards_finds_nearest_manifest() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let nested = repo_root.join("crates").join("foo");
        fs::create_dir_all(&nested)?;
        fs::write(repo_root.join("Cargo.toml"), "[workspace]\n")?;
        fs::write(
            nested.join("Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        )?;

        // act
        let dirs = find_cargo_manifests_upwards(&nested, &repo_root);

        // assert
        assert!(dirs.contains(&nested));
        assert!(dirs.contains(&repo_root));
        Ok(())
    }
}

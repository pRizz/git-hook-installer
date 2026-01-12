//! Hook status inspection and reporting.
//!
//! This module provides functionality to inspect the current state of git hooks
//! in a repository, including whether hooks are installed, their contents,
//! and whether they match expected hook scripts.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::hooks::{is_executable, MANAGED_BLOCK_BEGIN};

pub fn print_status(
    cwd: &Path,
    repo_root: &Path,
    git_dir: &Path,
    maybe_manifest_dir_from_cli: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");

    println!("Repository: {}", repo_root.display());
    println!("Git dir: {}", git_dir.display());
    println!("Hooks dir: {}", hooks_dir.display());

    if !hooks_dir.is_dir() {
        println!("Hooks dir status: missing");
        println!("pre-commit: not installed");
        return Ok(());
    }

    let (maybe_manifest_dir, manifest_note) =
        resolve_manifest_dir_for_status(cwd, repo_root, maybe_manifest_dir_from_cli)?;
    if let Some(note) = manifest_note {
        println!("{note}");
    }

    inspect_pre_commit(
        &hooks_dir,
        repo_root,
        maybe_manifest_dir.as_deref(),
        verbose,
    )?;
    Ok(())
}

fn resolve_manifest_dir_for_status(
    cwd: &Path,
    repo_root: &Path,
    maybe_manifest_dir_from_cli: Option<&Path>,
) -> Result<(Option<PathBuf>, Option<String>)> {
    let options = ResolveHookOptions {
        yes: true,
        non_interactive: true,
    };

    let result = resolve_cargo_manifest_dir(maybe_manifest_dir_from_cli, cwd, repo_root, options);
    let Ok(manifest_dir) = result else {
        return Ok((None, None));
    };

    Ok((
        Some(manifest_dir.clone()),
        Some(format!(
            "Cargo manifest dir (for comparison): {}",
            manifest_dir.display()
        )),
    ))
}

fn inspect_pre_commit(
    hooks_dir: &Path,
    _repo_root: &Path,
    _maybe_manifest_dir: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let hook_path = hooks_dir.join("pre-commit");
    if !hook_path.exists() {
        println!("pre-commit: not installed");
        print_hook_backups(hooks_dir, "pre-commit")?;
        return Ok(());
    }

    println!("pre-commit: installed");
    if let Some(is_executable) = is_executable(&hook_path) {
        println!("pre-commit executable: {is_executable}");
    }

    let Ok(contents) = fs::read_to_string(&hook_path) else {
        println!("pre-commit readable: false");
        print_hook_backups(hooks_dir, "pre-commit")?;
        return Ok(());
    };

    println!("pre-commit readable: true");

    let has_managed_block = contents.lines().any(|line| line.trim() == MANAGED_BLOCK_BEGIN);
    println!("pre-commit has git-hook-installer managed block: {has_managed_block}");

    let looks_like_cargo_fmt = contents.lines().any(|line| line.trim() == "cargo fmt");
    println!("pre-commit runs cargo fmt: {looks_like_cargo_fmt}");

    if let Some(cd_dir) = parse_cd_dir(&contents) {
        println!("pre-commit cd: {cd_dir}");
    }

    // Note: we no longer attempt to match an exact pre-commit hook script; we only report state.

    if verbose {
        print_hook_summary(&contents);
    }

    print_hook_backups(hooks_dir, "pre-commit")?;
    Ok(())
}

fn parse_cd_dir(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if !line.starts_with("cd ") {
            continue;
        }

        let raw = line.trim_start_matches("cd ").trim();
        let unquoted = raw
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(raw);
        return Some(unquoted.to_string());
    }
    None
}

fn print_hook_summary(contents: &str) {
    let line_count = contents.lines().count();
    println!("pre-commit lines: {line_count}");

    let has_shebang = contents
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("#!"));
    println!("pre-commit has shebang: {has_shebang}");
}

fn print_hook_backups(hooks_dir: &Path, hook_file_name: &str) -> Result<()> {
    let entries = match fs::read_dir(hooks_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    let prefix = format!("{hook_file_name}.bak");
    let mut backups = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with(&prefix) {
            continue;
        }
        backups.push(file_name.to_string());
    }

    backups.sort();
    if backups.is_empty() {
        return Ok(());
    }

    println!("pre-commit backups: {}", backups.join(", "));
    Ok(())
}

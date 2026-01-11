use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{collections::VecDeque, path::Component};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use dialoguer::{Confirm, Select};

#[derive(Debug, Parser)]
#[command(name = "git-hook-installer", version, about)]
struct Cli {
    /// Automatically answer "yes" to prompts
    #[arg(short = 'y', long)]
    yes: bool,

    /// Do not prompt; fail instead of asking questions
    #[arg(long)]
    non_interactive: bool,

    /// Overwrite existing hook files without prompting
    #[arg(short = 'f', long)]
    force: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Install a premade hook into the current repository
    Install {
        /// Hook to install
        #[arg(value_enum)]
        hook: Option<HookKind>,

        /// Directory containing the Cargo.toml to use (only used for cargo-fmt-pre-commit)
        #[arg(long, value_name = "DIR")]
        manifest_dir: Option<PathBuf>,
    },
    /// List available premade hooks
    List,
    /// Inspect and report current hook state for this repository
    Status {
        /// Directory containing the Cargo.toml to compare against (optional)
        #[arg(long, value_name = "DIR")]
        manifest_dir: Option<PathBuf>,

        /// Print more details (e.g. hook contents summary)
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HookKind {
    /// pre-commit hook that runs `cargo fmt`
    CargoFmtPreCommit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cwd = env::current_dir().context("Failed to read current working directory")?;
    let (repo_root, git_dir) = match find_git_repo(&cwd)? {
        Some(value) => value,
        None => {
            eprintln!("Not inside a git repository (no .git directory found).");
            return Ok(());
        }
    };

    match cli.command.unwrap_or(Command::Install {
        hook: None,
        manifest_dir: None,
    }) {
        Command::List => {
            println!("Available hooks:");
            println!("- cargo-fmt-pre-commit");
            return Ok(());
        }
        Command::Status {
            manifest_dir,
            verbose,
        } => {
            print_status(&cwd, &repo_root, &git_dir, manifest_dir.as_deref(), verbose)?;
        }
        Command::Install { hook, manifest_dir } => {
            let maybe_resolved_hook = resolve_hook_kind(
                hook,
                manifest_dir.as_deref(),
                &cwd,
                &repo_root,
                ResolveHookOptions {
                    yes: cli.yes,
                    non_interactive: cli.non_interactive,
                },
            )?;

            let Some(resolved_hook) = maybe_resolved_hook else {
                println!("No hook selected.");
                return Ok(());
            };

            install_hook(
                resolved_hook,
                &git_dir,
                InstallOptions {
                    yes: cli.yes,
                    non_interactive: cli.non_interactive,
                    force: cli.force,
                },
            )?;
        }
    }

    Ok(())
}

fn print_status(
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
    repo_root: &Path,
    maybe_manifest_dir: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let hook_path = hooks_dir.join("pre-commit");
    if !hook_path.exists() {
        println!("pre-commit: not installed");
        print_hook_backups(hooks_dir, "pre-commit")?;
        return Ok(());
    }

    let maybe_contents = fs::read_to_string(&hook_path).ok();
    let is_executable = is_executable(&hook_path);

    println!("pre-commit: installed");
    if let Some(is_executable) = is_executable {
        println!("pre-commit executable: {is_executable}");
    }

    let Some(contents) = maybe_contents else {
        println!("pre-commit readable: false");
        print_hook_backups(hooks_dir, "pre-commit")?;
        return Ok(());
    };

    println!("pre-commit readable: true");

    let looks_like_cargo_fmt = contents.lines().any(|line| line.trim() == "cargo fmt");
    println!("pre-commit runs cargo fmt: {looks_like_cargo_fmt}");

    if let Some(cd_dir) = parse_cd_dir(&contents) {
        println!("pre-commit cd: {cd_dir}");
    }

    if let Some(manifest_dir) = maybe_manifest_dir {
        let expected = cargo_fmt_pre_commit_script(manifest_dir);
        let is_exact_match = normalize_newlines(&contents) == normalize_newlines(&expected);
        println!(
            "pre-commit matches expected cargo-fmt hook: {is_exact_match} (manifest: {})",
            relative_display(repo_root, manifest_dir)
        );
    } else if looks_like_cargo_fmt {
        println!("pre-commit matches expected cargo-fmt hook: unknown (no manifest dir resolved)");
    }

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

fn normalize_newlines(s: &str) -> String {
    let mut normalized = s.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
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

#[cfg(unix)]
fn is_executable(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).ok()?;
    let mode = metadata.permissions().mode();
    Some((mode & 0o111) != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> Option<bool> {
    None
}

#[derive(Clone, Copy)]
struct ResolveHookOptions {
    yes: bool,
    non_interactive: bool,
}

#[derive(Debug, Clone)]
enum ResolvedHook {
    CargoFmtPreCommit { cargo_dir: PathBuf },
}

fn resolve_hook_kind(
    maybe_hook: Option<HookKind>,
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Result<Option<ResolvedHook>> {
    let is_explicit_hook = maybe_hook.is_some();
    let hook = maybe_hook.unwrap_or(HookKind::CargoFmtPreCommit);

    match hook {
        HookKind::CargoFmtPreCommit => {
            let cargo_dir_result =
                resolve_cargo_manifest_dir(maybe_manifest_dir_from_cli, cwd, repo_root, options);

            let cargo_dir = match cargo_dir_result {
                Ok(dir) => dir,
                Err(err) if !is_explicit_hook => {
                    println!(
                        "Detected git repository at {} but couldn't find a Cargo.toml to use.",
                        repo_root.display()
                    );
                    println!("Tip: if this is a monorepo, re-run with `--manifest-dir <dir>`.");
                    println!("Details: {err:#}");
                    return Ok(None);
                }
                Err(err) => return Err(err),
            };

            if options.non_interactive || options.yes {
                return Ok(Some(ResolvedHook::CargoFmtPreCommit { cargo_dir }));
            }

            let prompt = format!(
                "Install pre-commit hook to run `cargo fmt` (using Cargo.toml in {})?",
                cargo_dir.display()
            );
            let should_install = Confirm::new()
                .with_prompt(prompt)
                .default(true)
                .interact()
                .context("Failed to read confirmation from stdin")?;

            if !should_install {
                return Ok(None);
            }

            Ok(Some(ResolvedHook::CargoFmtPreCommit { cargo_dir }))
        }
    }
}

#[derive(Clone, Copy)]
struct InstallOptions {
    yes: bool,
    non_interactive: bool,
    force: bool,
}

fn install_hook(kind: ResolvedHook, git_dir: &Path, options: InstallOptions) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let (hook_name, hook_contents) = match kind {
        ResolvedHook::CargoFmtPreCommit { cargo_dir } => {
            ("pre-commit", cargo_fmt_pre_commit_script(&cargo_dir))
        }
    };

    let hook_path = hooks_dir.join(hook_name);
    write_hook_file(&hook_path, hook_contents.as_bytes(), options)?;

    println!("Installed `{}` hook at {}", hook_name, hook_path.display());
    Ok(())
}

fn cargo_fmt_pre_commit_script(cargo_dir: &Path) -> String {
    format!(
        r#"#!/bin/sh
set -e

cd "{}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found; skipping cargo fmt"
  exit 0
fi

echo "Running cargo fmt..."
cargo fmt

"#,
        shell_escape_path(cargo_dir)
    )
}

fn shell_escape_path(path: &Path) -> String {
    // Minimal escaping for POSIX sh: wrap in double quotes and escape embedded quotes/backslashes.
    let raw = path.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len() + 2);
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn write_hook_file(path: &Path, contents: &[u8], options: InstallOptions) -> Result<()> {
    if path.exists() {
        handle_existing_hook(path, options)?;
    }

    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create hook file at {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("Failed to write hook file at {}", path.display()))?;

    set_executable(path)
        .with_context(|| format!("Failed to mark {} as executable", path.display()))?;
    Ok(())
}

fn handle_existing_hook(path: &Path, options: InstallOptions) -> Result<()> {
    if options.force || options.yes {
        return backup_existing_hook(path);
    }

    if options.non_interactive {
        return Err(anyhow!(
            "Hook already exists at {} (use --force to overwrite)",
            path.display()
        ));
    }

    println!("Hook already exists at {}.", path.display());
    let should_overwrite = Confirm::new()
        .with_prompt("Back up existing hook and overwrite?")
        .default(false)
        .interact()
        .context("Failed to read confirmation from stdin")?;

    if !should_overwrite {
        return Err(anyhow!("Aborted (existing hook was not modified)."));
    }

    backup_existing_hook(path)
}

fn backup_existing_hook(path: &Path) -> Result<()> {
    let maybe_file_name = path.file_name().and_then(OsStr::to_str);
    let Some(file_name) = maybe_file_name else {
        return Err(anyhow!("Invalid hook path: {}", path.display()));
    };

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid hook path (no parent): {}", path.display()))?;

    let mut counter: u32 = 0;
    loop {
        let suffix = if counter == 0 {
            ".bak".to_string()
        } else {
            format!(".bak.{}", counter)
        };

        let backup_path = parent.join(format!("{}{}", file_name, suffix));
        if !backup_path.exists() {
            fs::copy(path, &backup_path).with_context(|| {
                format!(
                    "Failed to back up existing hook from {} to {}",
                    path.display(),
                    backup_path.display()
                )
            })?;
            println!("Backed up existing hook to {}", backup_path.display());
            return Ok(());
        }

        counter = counter.saturating_add(1);
        if counter > 10_000 {
            return Err(anyhow!(
                "Too many backup files exist for {}",
                path.display()
            ));
        }
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Finds the nearest git repository by walking parents looking for `.git`.
/// Returns (repo_root, git_dir_path).
fn find_git_repo(start: &Path) -> Result<Option<(PathBuf, PathBuf)>> {
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

fn resolve_cargo_manifest_dir(
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Result<PathBuf> {
    if let Some(manifest_dir) = maybe_manifest_dir_from_cli {
        let manifest_dir = normalize_path(repo_root, manifest_dir)?;
        ensure_is_within_repo(repo_root, &manifest_dir)?;

        let cargo_toml = manifest_dir.join("Cargo.toml");
        if !cargo_toml.is_file() {
            return Err(anyhow!(
                "--manifest-dir {} does not contain a Cargo.toml",
                manifest_dir.display()
            ));
        }

        return Ok(manifest_dir);
    }

    let mut manifest_dirs = Vec::new();
    manifest_dirs.extend(find_cargo_manifests_upwards(cwd, repo_root));
    if manifest_dirs.is_empty() {
        manifest_dirs = find_cargo_manifests_bfs(repo_root, 6, 8_000)?;
    }

    if manifest_dirs.is_empty() {
        return Err(anyhow!(
            "No Cargo.toml found in git repository at {}",
            repo_root.display()
        ));
    }

    manifest_dirs.sort();
    manifest_dirs.dedup();

    if manifest_dirs.len() == 1 {
        let Some(only_dir) = manifest_dirs.into_iter().next() else {
            return Err(anyhow!("Internal error resolving manifest directories"));
        };
        return Ok(only_dir);
    }

    if options.non_interactive || options.yes {
        return Err(anyhow!(
            "Multiple Cargo.toml files found; re-run with --manifest-dir to choose one"
        ));
    }

    let mut labels = Vec::with_capacity(manifest_dirs.len());
    for dir in &manifest_dirs {
        let label = relative_display(repo_root, dir);
        labels.push(label);
    }

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

fn normalize_path(repo_root: &Path, input: &Path) -> Result<PathBuf> {
    if input.is_absolute() {
        return Ok(input.to_path_buf());
    }
    Ok(repo_root.join(input))
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

fn find_cargo_manifests_upwards(cwd: &Path, repo_root: &Path) -> Vec<PathBuf> {
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

fn relative_display(base: &Path, path: &Path) -> String {
    let maybe_rel = path.strip_prefix(base).ok();
    let Some(rel) = maybe_rel else {
        return path.display().to_string();
    };
    if rel.as_os_str().is_empty() {
        ".".to_string()
    } else {
        rel.display().to_string()
    }
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

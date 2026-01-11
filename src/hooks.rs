use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use dialoguer::Confirm;

#[derive(Clone, Copy)]
pub struct InstallOptions {
    pub yes: bool,
    pub non_interactive: bool,
    pub force: bool,
}

pub fn install_hook_script(
    git_dir: &Path,
    hook_name: &str,
    hook_contents: &str,
    options: InstallOptions,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let hook_path = hooks_dir.join(hook_name);
    write_hook_file(&hook_path, hook_contents.as_bytes(), options)?;

    println!("Installed `{}` hook at {}", hook_name, hook_path.display());
    Ok(())
}

pub fn cargo_fmt_pre_commit_script(cargo_dir: &Path) -> String {
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
    // Minimal escaping for POSIX sh: wrap in double quotes and escape embedded quotes/backslashes,
    // dollar signs, and backticks to prevent command injection.
    let raw = path.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len() + 2);
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '`' => escaped.push_str("\\`"),
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
        if backup_path.exists() {
            counter = counter.saturating_add(1);
            if counter > 10_000 {
                return Err(anyhow!(
                    "Too many backup files exist for {}",
                    path.display()
                ));
            }
            continue;
        }

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

#[cfg(unix)]
pub fn is_executable(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).ok()?;
    let mode = metadata.permissions().mode();
    Some((mode & 0o111) != 0)
}

#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> Option<bool> {
    None
}

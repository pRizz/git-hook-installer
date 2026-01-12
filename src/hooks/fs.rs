use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use dialoguer::Confirm;

use crate::hooks::managed_block::{ensure_shebang, MANAGED_BLOCK_BEGIN, MANAGED_BLOCK_END};
use crate::hooks::snapshots::{create_hook_snapshot_and_prune, DEFAULT_MAX_SNAPSHOTS};
use crate::hooks::types::InstallOptions;

pub fn upsert_managed_block_in_file(path: &Path, block: &str, options: InstallOptions) -> Result<()> {
    let existing = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read hook file at {}", path.display()))?;
        Some(contents)
    } else {
        None
    };

    let updated = match existing.as_deref() {
        None => ensure_shebang(block),
        Some(contents) => {
            let has_managed = contents.contains(MANAGED_BLOCK_BEGIN) && contents.contains(MANAGED_BLOCK_END);
            if !has_managed {
                // This is an existing user hook; get consent and back it up before modifying.
                handle_existing_hook(path, options)?;
            }
            crate::hooks::managed_block::upsert_managed_block(contents, block)
        }
    };

    if let Some(existing) = existing.as_deref() {
        if existing == updated {
            // No changes to write; do not create a snapshot.
            return Ok(());
        }
        create_hook_snapshot_and_prune(path, DEFAULT_MAX_SNAPSHOTS)?;
    }

    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create hook file at {}", path.display()))?;
    file.write_all(updated.as_bytes())
        .with_context(|| format!("Failed to write hook file at {}", path.display()))?;
    Ok(())
}

pub fn write_hook_with_snapshot_if_changed(path: &Path, existing: &str, updated: &str) -> Result<()> {
    if existing == updated {
        return Ok(());
    }

    create_hook_snapshot_and_prune(path, DEFAULT_MAX_SNAPSHOTS)?;
    fs::write(path, updated.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn handle_existing_hook(path: &Path, options: InstallOptions) -> Result<()> {
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
pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn set_executable(_path: &Path) -> Result<()> {
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


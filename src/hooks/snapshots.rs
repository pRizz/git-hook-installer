use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use time::{format_description, OffsetDateTime};

pub const DEFAULT_MAX_SNAPSHOTS: usize = 10;

pub fn create_hook_snapshot_and_prune(hook_path: &Path, max_snapshots: usize) -> Result<()> {
    if !hook_path.is_file() {
        return Ok(());
    }

    let file_name = hook_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("Invalid hook path: {}", hook_path.display()))?;

    let parent = hook_path
        .parent()
        .ok_or_else(|| anyhow!("Invalid hook path (no parent): {}", hook_path.display()))?;

    let timestamp = format_timestamp_for_snapshot_name(OffsetDateTime::now_utc())?;
    let prefix = format!("{file_name}.snapshot-");
    let mut snapshot_path = parent.join(format!("{prefix}{timestamp}"));

    // Extremely unlikely, but ensure uniqueness.
    let mut counter: u32 = 0;
    while snapshot_path.exists() {
        counter = counter.saturating_add(1);
        if counter > 10_000 {
            return Err(anyhow!(
                "Too many snapshot files exist for {}",
                hook_path.display()
            ));
        }
        snapshot_path = parent.join(format!("{prefix}{timestamp}.{counter}"));
    }

    fs::copy(hook_path, &snapshot_path).with_context(|| {
        format!(
            "Failed to snapshot existing hook from {} to {}",
            hook_path.display(),
            snapshot_path.display()
        )
    })?;

    println!("Created snapshot of existing hook at {}", snapshot_path.display());

    prune_hook_snapshots(parent, &prefix, max_snapshots)?;
    Ok(())
}

pub fn prune_hook_snapshots(hooks_dir: &Path, prefix: &str, max_snapshots: usize) -> Result<()> {
    if max_snapshots == 0 {
        return Ok(());
    }

    let entries = fs::read_dir(hooks_dir).with_context(|| {
        format!(
            "Failed to list hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let mut snapshots: Vec<String> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with(prefix) {
            continue;
        }
        snapshots.push(file_name.to_string());
    }

    // Lexicographic order matches chronological order for our timestamp format.
    snapshots.sort();

    if snapshots.len() <= max_snapshots {
        return Ok(());
    }

    let remove_count = snapshots.len() - max_snapshots;
    for file_name in snapshots.into_iter().take(remove_count) {
        let path = hooks_dir.join(&file_name);
        let _ = fs::remove_file(&path);
    }

    Ok(())
}

fn format_timestamp_for_snapshot_name(dt: OffsetDateTime) -> Result<String> {
    let fmt = format_description::parse("[year]-[month]-[day]-[hour]-[minute]-[second]")
        .context("Failed to build timestamp format")?;
    let timestamp = dt.format(&fmt).context("Failed to format timestamp")?;
    Ok(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn snapshot_prune_keeps_newest_10() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let hooks_dir = temp.path();
        let hook_path = hooks_dir.join("pre-commit");
        fs::write(&hook_path, "old\n")?;

        // Create 12 fake snapshots; pruning should keep 10.
        for i in 0..12 {
            let name = format!("pre-commit.snapshot-2026-01-11-15-{:02}-{:02}", i, 0);
            fs::write(hooks_dir.join(name), "snap\n")?;
        }

        // act
        create_hook_snapshot_and_prune(&hook_path, 10)?;

        // assert
        let mut snapshot_count = 0usize;
        for entry in fs::read_dir(hooks_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with("pre-commit.snapshot-") {
                snapshot_count += 1;
            }
        }
        assert_eq!(snapshot_count, 10);
        Ok(())
    }
}


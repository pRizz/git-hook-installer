//! Git hook installation and script generation.
//!
//! Public API lives in this file (`hooks.rs`), with implementation split into
//! `hooks/` submodules for maintainability.

mod fs;
mod managed_block;
mod script;
mod snapshots;
mod types;

use std::fs as stdfs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

pub use fs::is_executable;
pub use managed_block::MANAGED_BLOCK_BEGIN;
pub use script::managed_pre_commit_block;
pub use types::{InstallOptions, JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool};

pub const PRE_COMMIT_HOOK_NAME: &str = "pre-commit";

pub fn upsert_managed_pre_commit_hook(
    git_dir: &Path,
    block: &str,
    options: InstallOptions,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    stdfs::create_dir_all(&hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let hook_path = hooks_dir.join(PRE_COMMIT_HOOK_NAME);
    fs::upsert_managed_block_in_file(&hook_path, block, options)?;
    fs::set_executable(&hook_path)
        .with_context(|| format!("Failed to mark {} as executable", hook_path.display()))?;
    println!(
        "Installed `{}` hook at {}",
        PRE_COMMIT_HOOK_NAME,
        hook_path.display()
    );
    Ok(())
}

pub fn disable_managed_pre_commit_hook(git_dir: &Path) -> Result<()> {
    let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
    if !hook_path.exists() {
        return Err(anyhow!(
            "No pre-commit hook exists at {}",
            hook_path.display()
        ));
    }

    let contents = stdfs::read_to_string(&hook_path)
        .with_context(|| format!("Failed to read {}", hook_path.display()))?;
    let updated = managed_block::disable_managed_block(&contents)?;
    fs::write_hook_with_snapshot_if_changed(&hook_path, &contents, &updated)?;
    println!(
        "Disabled managed git-hook-installer block in {}",
        hook_path.display()
    );
    Ok(())
}

pub fn uninstall_managed_pre_commit_hook(git_dir: &Path) -> Result<()> {
    let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
    if !hook_path.exists() {
        return Err(anyhow!(
            "No pre-commit hook exists at {}",
            hook_path.display()
        ));
    }

    let contents = stdfs::read_to_string(&hook_path)
        .with_context(|| format!("Failed to read {}", hook_path.display()))?;
    let updated = managed_block::uninstall_managed_block(&contents)?;

    if updated.trim().is_empty() {
        snapshots::create_hook_snapshot_and_prune(&hook_path, snapshots::DEFAULT_MAX_SNAPSHOTS)?;
        stdfs::remove_file(&hook_path)
            .with_context(|| format!("Failed to remove {}", hook_path.display()))?;
        println!("Removed {}", hook_path.display());
        return Ok(());
    }

    fs::write_hook_with_snapshot_if_changed(&hook_path, &contents, &updated)?;
    println!(
        "Uninstalled managed git-hook-installer block in {}",
        hook_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn upsert_managed_pre_commit_hook_writes_file() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(git_dir.join("hooks"))?;

        let settings = ManagedPreCommitSettings {
            enabled: true,
            maybe_js_ts_tool: Some(JsTsTool::Biome),
            ts_typecheck_enabled: true,
            maybe_python_tool: Some(PythonTool::Ruff),
            maybe_java_kotlin_tool: Some(JavaKotlinTool::Spotless),
            go_enabled: true,
            shell_enabled: true,
            terraform_enabled: true,
            c_cpp_enabled: true,
            ruby_enabled: true,
            maybe_cargo_manifest_dir: None,
        };
        let repo_root = temp.path();
        let block = managed_pre_commit_block(&settings, repo_root);

        // act
        upsert_managed_pre_commit_hook(
            &git_dir,
            &block,
            InstallOptions {
                yes: true,
                non_interactive: true,
                force: true,
            },
        )?;

        // assert
        let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
        assert!(hook_path.is_file());
        Ok(())
    }
}

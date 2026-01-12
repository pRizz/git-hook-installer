//! Hook resolution and installation orchestration.
//!
//! This module coordinates the process of resolving which hook to install
//! (including user prompts when needed) and then installing the resolved hook
//! into the git repository.

use std::path::Path;

use anyhow::{Context, Result};
use dialoguer::Confirm;

use crate::cargo_repo::ResolveHookOptions;
use crate::cli::HookKind;
use crate::hooks::{
    disable_managed_pre_commit_hook, managed_pre_commit_block, uninstall_managed_pre_commit_hook,
    upsert_managed_pre_commit_hook, InstallOptions, ManagedPreCommitSettings,
};

mod detect;
mod prompts;

#[derive(Debug, Clone)]
pub enum ResolvedHook {
    PreCommit { settings: ManagedPreCommitSettings },
}

pub fn resolve_hook_kind(
    maybe_hook: Option<HookKind>,
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Result<Option<ResolvedHook>> {
    let hook = maybe_hook.unwrap_or(HookKind::PreCommit);

    match hook {
        HookKind::PreCommit => {
            let maybe_cargo_dir = detect::resolve_cargo_dir_best_effort(
                maybe_manifest_dir_from_cli,
                cwd,
                repo_root,
                ResolveHookOptions {
                    yes: true,
                    non_interactive: true,
                },
            );

            let settings = prompts::resolve_pre_commit_settings(repo_root, maybe_cargo_dir, options)?;

            if options.non_interactive || options.yes {
                return Ok(Some(ResolvedHook::PreCommit { settings }));
            }

            let prompt = "Install/update managed `pre-commit` hook (formatters/linters + safe stash/rollback)?".to_string();
            let should_install = Confirm::new()
                .with_prompt(prompt)
                .default(true)
                .interact()
                .context("Failed to read confirmation from stdin")?;

            if !should_install {
                return Ok(None);
            }

            Ok(Some(ResolvedHook::PreCommit { settings }))
        }
    }
}

pub fn install_resolved_hook(
    kind: ResolvedHook,
    git_dir: &Path,
    repo_root: &Path,
    options: InstallOptions,
) -> Result<()> {
    match kind {
        ResolvedHook::PreCommit { settings } => {
            // Note: settings are stored inside the managed block itself (no repo config).
            // We still want the managed block to have an absolute manifest dir if present.
            let block = managed_pre_commit_block(&settings, &repo_root);
            upsert_managed_pre_commit_hook(git_dir, &block, options)
        }
    }
}

pub fn disable_managed_pre_commit(git_dir: &Path) -> Result<()> {
    disable_managed_pre_commit_hook(git_dir)
}

pub fn uninstall_managed_pre_commit(git_dir: &Path) -> Result<()> {
    uninstall_managed_pre_commit_hook(git_dir)
}

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cargo_repo::ResolveHookOptions;
use crate::hooks::ManagedPreCommitSettings;

use super::detect::{default_java_kotlin_tool, default_js_ts_tool, default_python_tool};

pub fn resolve_pre_commit_settings(
    repo_root: &Path,
    maybe_cargo_dir: Option<PathBuf>,
    options: ResolveHookOptions,
) -> Result<ManagedPreCommitSettings> {
    let default_js_ts = default_js_ts_tool(repo_root);
    let default_python = default_python_tool(repo_root);
    let default_java_kotlin = default_java_kotlin_tool(repo_root);

    // We intentionally avoid prompting for toolchain selection:
    // - The managed hook auto-skips when there are no matching staged files.
    // - Each tool is skipped if it isn't available on PATH (or via npx where applicable).
    // - Repo settings are stored in the hook file; users can uninstall/reinstall to re-detect.
    //
    // If callers want "no questions asked", they can still use --yes / --non-interactive,
    // but the toolchain selection itself is always non-interactive.
    let _ = options;

    Ok(ManagedPreCommitSettings {
        enabled: true,
        js_ts_tool: default_js_ts,
        python_tool: default_python,
        java_kotlin_tool: default_java_kotlin,
        maybe_cargo_manifest_dir: maybe_cargo_dir,
    })
}


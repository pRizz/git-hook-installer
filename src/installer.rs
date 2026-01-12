//! Hook resolution and installation orchestration.
//!
//! This module coordinates the process of resolving which hook to install
//! (including user prompts when needed) and then installing the resolved hook
//! into the git repository.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dialoguer::{Confirm, Select};

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::cli::HookKind;
use crate::hooks::{
    cargo_fmt_pre_commit_script, disable_managed_pre_commit_hook, install_hook_script,
    managed_pre_commit_block, uninstall_managed_pre_commit_hook, upsert_managed_pre_commit_hook,
    InstallOptions, JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool,
};

#[derive(Debug, Clone)]
pub enum ResolvedHook {
    PreCommit { settings: ManagedPreCommitSettings },
    CargoFmtPreCommit { cargo_dir: PathBuf },
}

pub fn resolve_hook_kind(
    maybe_hook: Option<HookKind>,
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Result<Option<ResolvedHook>> {
    let is_explicit_hook = maybe_hook.is_some();
    let hook = maybe_hook.unwrap_or(HookKind::PreCommit);

    match hook {
        HookKind::PreCommit => {
            let maybe_cargo_dir = resolve_cargo_dir_best_effort(
                maybe_manifest_dir_from_cli,
                cwd,
                repo_root,
                ResolveHookOptions {
                    yes: true,
                    non_interactive: true,
                },
            );

            let settings = resolve_pre_commit_settings(repo_root, maybe_cargo_dir, options)?;

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
        ResolvedHook::CargoFmtPreCommit { cargo_dir } => {
            let script = cargo_fmt_pre_commit_script(&cargo_dir);
            install_hook_script(git_dir, "pre-commit", &script, options)
        }
    }
}

pub fn disable_managed_pre_commit(git_dir: &Path) -> Result<()> {
    disable_managed_pre_commit_hook(git_dir)
}

pub fn uninstall_managed_pre_commit(git_dir: &Path) -> Result<()> {
    uninstall_managed_pre_commit_hook(git_dir)
}

fn resolve_cargo_dir_best_effort(
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Option<PathBuf> {
    let result = resolve_cargo_manifest_dir(maybe_manifest_dir_from_cli, cwd, repo_root, options);
    let Ok(cargo_dir) = result else {
        return None;
    };
    Some(cargo_dir)
}

fn resolve_pre_commit_settings(
    repo_root: &Path,
    maybe_cargo_dir: Option<PathBuf>,
    options: ResolveHookOptions,
) -> Result<ManagedPreCommitSettings> {
    let default_js_ts = default_js_ts_tool(repo_root);
    let default_python = PythonTool::Ruff;
    let default_java_kotlin = default_java_kotlin_tool(repo_root);

    if options.non_interactive || options.yes {
        return Ok(ManagedPreCommitSettings {
            enabled: true,
            js_ts_tool: default_js_ts,
            python_tool: default_python,
            java_kotlin_tool: default_java_kotlin,
            maybe_cargo_manifest_dir: maybe_cargo_dir,
        });
    }

    let js_ts_tool = Select::new()
        .with_prompt("JS/TS: choose formatter/linter toolchain")
        .default(match default_js_ts {
            JsTsTool::Biome => 0,
            JsTsTool::PrettierEslint => 1,
        })
        .items(&["Biome (check --write)", "Prettier (write) + ESLint (--fix)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let js_ts_tool = match js_ts_tool {
        0 => JsTsTool::Biome,
        _ => JsTsTool::PrettierEslint,
    };

    let python_tool = Select::new()
        .with_prompt("Python: choose formatter/linter toolchain")
        .default(0)
        .items(&["ruff (format + check --fix)", "black (format only)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let python_tool = match python_tool {
        0 => PythonTool::Ruff,
        _ => PythonTool::Black,
    };

    let java_kotlin_tool = Select::new()
        .with_prompt("Java/Kotlin: choose formatter toolchain")
        .default(match default_java_kotlin {
            JavaKotlinTool::Spotless => 0,
            JavaKotlinTool::Ktlint => 1,
        })
        .items(&["Spotless (gradle spotlessApply)", "ktlint (-F on staged Kotlin files)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let java_kotlin_tool = match java_kotlin_tool {
        0 => JavaKotlinTool::Spotless,
        _ => JavaKotlinTool::Ktlint,
    };

    Ok(ManagedPreCommitSettings {
        enabled: true,
        js_ts_tool,
        python_tool,
        java_kotlin_tool,
        maybe_cargo_manifest_dir: maybe_cargo_dir,
    })
}

fn default_js_ts_tool(repo_root: &Path) -> JsTsTool {
    // Prefer Biome if a biome config exists; otherwise prefer Prettier/Eslint.
    let has_biome = repo_root.join("biome.json").is_file() || repo_root.join("biome.jsonc").is_file();
    if has_biome {
        return JsTsTool::Biome;
    }
    JsTsTool::PrettierEslint
}

fn default_java_kotlin_tool(repo_root: &Path) -> JavaKotlinTool {
    // Prefer Spotless if this looks like a Gradle project.
    let has_gradle = repo_root.join("gradlew").is_file()
        || repo_root.join("build.gradle").is_file()
        || repo_root.join("build.gradle.kts").is_file();
    if has_gradle {
        return JavaKotlinTool::Spotless;
    }
    JavaKotlinTool::Ktlint
}

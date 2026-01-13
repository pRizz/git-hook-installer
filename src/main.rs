//! Main entry point for the git-hook-installer CLI tool.
//!
//! This module handles command-line parsing and orchestrates the installation
//! or status checking of git hooks in the current repository.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use dialoguer::Confirm;

mod cargo_repo;
mod cli;
mod git_repo;
mod hooks;
mod installer;
mod status;
mod util;

use crate::cargo_repo::ResolveHookOptions;
use crate::cli::{Cli, Command, HookKind};
use crate::git_repo::{find_git_repo, find_git_repos_under_dir};
use crate::hooks::InstallOptions;
use crate::installer::{
    disable_managed_pre_commit, disable_managed_pre_commit_best_effort, install_resolved_hook,
    resolve_hook_kind, uninstall_managed_pre_commit, uninstall_managed_pre_commit_best_effort,
};
use crate::status::print_status;

fn install_in_repo(
    cwd: &Path,
    repo_root: &Path,
    git_dir: &Path,
    hook: Option<HookKind>,
    manifest_dir: Option<PathBuf>,
    resolve_options: ResolveHookOptions,
    install_options: InstallOptions,
) -> Result<()> {
    let maybe_resolved_hook = resolve_hook_kind(
        hook,
        manifest_dir.as_deref(),
        cwd,
        repo_root,
        resolve_options,
    )?;

    let Some(resolved_hook) = maybe_resolved_hook else {
        println!("No hook selected.");
        return Ok(());
    };

    install_resolved_hook(resolved_hook, git_dir, repo_root, install_options)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cwd = env::current_dir().context("Failed to read current working directory")?;
    let command = cli.command.unwrap_or(Command::Install {
        hook: None,
        manifest_dir: None,
        recursive: false,
        max_depth: None,
        dir: None,
    });

    match command {
        Command::List => {
            println!("Available hooks:");
            println!("- pre-commit");
            Ok(())
        }
        Command::Install {
            hook,
            manifest_dir,
            recursive,
            max_depth,
            dir,
        } => {
            let scan_mode = recursive || dir.is_some() || max_depth.is_some();
            if scan_mode {
                let effective_max_depth = max_depth.unwrap_or(if recursive { 1 } else { 0 });
                let scan_root = dir.unwrap_or(cwd);
                println!(
                    "Scanning {} for git repositories (max depth: {})",
                    scan_root.display(),
                    effective_max_depth
                );
                let repos = find_git_repos_under_dir(&scan_root, effective_max_depth)?;
                if repos.is_empty() {
                    println!("No git repositories found under {}", scan_root.display());
                    return Ok(());
                }

                if cli.non_interactive && !cli.yes {
                    anyhow::bail!(
                        "Refusing to run scan-mode install without confirmation (found {} repos). Re-run with --yes.",
                        repos.len()
                    );
                }

                if !cli.yes && !cli.non_interactive {
                    println!(
                        "Found {} git repositories under {}:",
                        repos.len(),
                        scan_root.display()
                    );
                    let preview_limit = 25usize;
                    for (idx, (repo_root, _)) in repos.iter().take(preview_limit).enumerate() {
                        println!("  {:>2}. {}", idx + 1, repo_root.display());
                    }
                    if repos.len() > preview_limit {
                        println!("  ... and {} more", repos.len() - preview_limit);
                    }

                    let should_continue = Confirm::new()
                        .with_prompt(format!("Run installer in {} repositories?", repos.len()))
                        .default(false)
                        .interact()
                        .context("Failed to read confirmation from stdin")?;

                    if !should_continue {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
                for (repo_root, git_dir) in repos {
                    println!("\n==> {}", repo_root.display());
                    let result = install_in_repo(
                        &repo_root,
                        &repo_root,
                        &git_dir,
                        hook,
                        manifest_dir.clone(),
                        // After the global confirmation, don't ask the per-repo "install?" prompt.
                        ResolveHookOptions {
                            yes: true,
                            non_interactive: cli.non_interactive,
                        },
                        InstallOptions {
                            yes: cli.yes,
                            non_interactive: cli.non_interactive,
                            force: cli.force,
                        },
                    );
                    if let Err(err) = result {
                        eprintln!("Failed in {}: {err:#}", repo_root.display());
                        failures.push((repo_root, err));
                    }
                }

                if failures.is_empty() {
                    return Ok(());
                }

                anyhow::bail!(
                    "Scan-mode install completed with {} failure(s).",
                    failures.len()
                )
            } else {
                let (repo_root, git_dir) = match find_git_repo(&cwd)? {
                    Some(value) => value,
                    None => {
                        eprintln!("Not inside a git repository (no .git directory found).");
                        return Ok(());
                    }
                };

                install_in_repo(
                    &cwd,
                    &repo_root,
                    &git_dir,
                    hook,
                    manifest_dir,
                    ResolveHookOptions {
                        yes: cli.yes,
                        non_interactive: cli.non_interactive,
                    },
                    InstallOptions {
                        yes: cli.yes,
                        non_interactive: cli.non_interactive,
                        force: cli.force,
                    },
                )
            }
        }
        Command::Disable {
            recursive,
            max_depth,
            dir,
        } => {
            let scan_mode = recursive || dir.is_some() || max_depth.is_some();
            if scan_mode {
                let effective_max_depth = max_depth.unwrap_or(if recursive { 1 } else { 0 });
                let scan_root = dir.unwrap_or(cwd);
                println!(
                    "Scanning {} for git repositories (max depth: {})",
                    scan_root.display(),
                    effective_max_depth
                );
                let repos = find_git_repos_under_dir(&scan_root, effective_max_depth)?;
                if repos.is_empty() {
                    println!("No git repositories found under {}", scan_root.display());
                    return Ok(());
                }

                if cli.non_interactive && !cli.yes {
                    anyhow::bail!(
                        "Refusing to run scan-mode disable without confirmation (found {} repos). Re-run with --yes.",
                        repos.len()
                    );
                }

                if !cli.yes && !cli.non_interactive {
                    println!(
                        "Found {} git repositories under {}:",
                        repos.len(),
                        scan_root.display()
                    );
                    let preview_limit = 25usize;
                    for (idx, (repo_root, _)) in repos.iter().take(preview_limit).enumerate() {
                        println!("  {:>2}. {}", idx + 1, repo_root.display());
                    }
                    if repos.len() > preview_limit {
                        println!("  ... and {} more", repos.len() - preview_limit);
                    }

                    let should_continue = Confirm::new()
                        .with_prompt(format!("Run disable in {} repositories?", repos.len()))
                        .default(false)
                        .interact()
                        .context("Failed to read confirmation from stdin")?;

                    if !should_continue {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
                for (repo_root, git_dir) in repos {
                    println!("\n==> {}", repo_root.display());
                    let result = disable_managed_pre_commit_best_effort(&git_dir);
                    if let Err(err) = result {
                        eprintln!("Failed in {}: {err:#}", repo_root.display());
                        failures.push((repo_root, err));
                    }
                }

                if failures.is_empty() {
                    return Ok(());
                }

                anyhow::bail!(
                    "Scan-mode disable completed with {} failure(s).",
                    failures.len()
                )
            } else {
                let (_repo_root, git_dir) = match find_git_repo(&cwd)? {
                    Some(value) => value,
                    None => {
                        eprintln!("Not inside a git repository (no .git directory found).");
                        return Ok(());
                    }
                };
                disable_managed_pre_commit(&git_dir)
            }
        }
        Command::Uninstall {
            recursive,
            max_depth,
            dir,
        } => {
            let scan_mode = recursive || dir.is_some() || max_depth.is_some();
            if scan_mode {
                let effective_max_depth = max_depth.unwrap_or(if recursive { 1 } else { 0 });
                let scan_root = dir.unwrap_or(cwd);
                println!(
                    "Scanning {} for git repositories (max depth: {})",
                    scan_root.display(),
                    effective_max_depth
                );
                let repos = find_git_repos_under_dir(&scan_root, effective_max_depth)?;
                if repos.is_empty() {
                    println!("No git repositories found under {}", scan_root.display());
                    return Ok(());
                }

                if cli.non_interactive && !cli.yes {
                    anyhow::bail!(
                        "Refusing to run scan-mode uninstall without confirmation (found {} repos). Re-run with --yes.",
                        repos.len()
                    );
                }

                if !cli.yes && !cli.non_interactive {
                    println!(
                        "Found {} git repositories under {}:",
                        repos.len(),
                        scan_root.display()
                    );
                    let preview_limit = 25usize;
                    for (idx, (repo_root, _)) in repos.iter().take(preview_limit).enumerate() {
                        println!("  {:>2}. {}", idx + 1, repo_root.display());
                    }
                    if repos.len() > preview_limit {
                        println!("  ... and {} more", repos.len() - preview_limit);
                    }

                    let should_continue = Confirm::new()
                        .with_prompt(format!("Run uninstall in {} repositories?", repos.len()))
                        .default(false)
                        .interact()
                        .context("Failed to read confirmation from stdin")?;

                    if !should_continue {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
                for (repo_root, git_dir) in repos {
                    println!("\n==> {}", repo_root.display());
                    let result = uninstall_managed_pre_commit_best_effort(&git_dir);
                    if let Err(err) = result {
                        eprintln!("Failed in {}: {err:#}", repo_root.display());
                        failures.push((repo_root, err));
                    }
                }

                if failures.is_empty() {
                    return Ok(());
                }

                anyhow::bail!(
                    "Scan-mode uninstall completed with {} failure(s).",
                    failures.len()
                )
            } else {
                let (_repo_root, git_dir) = match find_git_repo(&cwd)? {
                    Some(value) => value,
                    None => {
                        eprintln!("Not inside a git repository (no .git directory found).");
                        return Ok(());
                    }
                };
                uninstall_managed_pre_commit(&git_dir)
            }
        }
        Command::Status {
            verbose,
            recursive,
            max_depth,
            dir,
        } => {
            let scan_mode = recursive || dir.is_some() || max_depth.is_some();
            if scan_mode {
                let effective_max_depth = max_depth.unwrap_or(if recursive { 1 } else { 0 });
                let scan_root = dir.unwrap_or(cwd);
                println!(
                    "Scanning {} for git repositories (max depth: {})",
                    scan_root.display(),
                    effective_max_depth
                );
                let repos = find_git_repos_under_dir(&scan_root, effective_max_depth)?;
                if repos.is_empty() {
                    println!("No git repositories found under {}", scan_root.display());
                    return Ok(());
                }

                let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
                for (repo_root, git_dir) in repos {
                    println!("\n==> {}", repo_root.display());
                    let result = print_status(&repo_root, &git_dir, verbose);
                    if let Err(err) = result {
                        eprintln!("Failed in {}: {err:#}", repo_root.display());
                        failures.push((repo_root, err));
                    }
                }

                if failures.is_empty() {
                    return Ok(());
                }

                anyhow::bail!(
                    "Scan-mode status completed with {} failure(s).",
                    failures.len()
                )
            } else {
                let (repo_root, git_dir) = match find_git_repo(&cwd)? {
                    Some(value) => value,
                    None => {
                        eprintln!("Not inside a git repository (no .git directory found).");
                        return Ok(());
                    }
                };
                print_status(&repo_root, &git_dir, verbose)
            }
        }
    }
}

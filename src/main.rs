//! Main entry point for the git-hook-installer CLI tool.
//!
//! This module handles command-line parsing and orchestrates the installation
//! or status checking of git hooks in the current repository.

use std::env;

use anyhow::{Context, Result};
use clap::Parser;

mod cargo_repo;
mod cli;
mod git_repo;
mod hooks;
mod installer;
mod status;
mod util;

use crate::cargo_repo::ResolveHookOptions;
use crate::cli::{Cli, Command};
use crate::git_repo::find_git_repo;
use crate::hooks::InstallOptions;
use crate::installer::{install_resolved_hook, resolve_hook_kind};
use crate::status::print_status;

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
            Ok(())
        }
        Command::Status {
            manifest_dir,
            verbose,
        } => print_status(&cwd, &repo_root, &git_dir, manifest_dir.as_deref(), verbose),
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

            install_resolved_hook(
                resolved_hook,
                &git_dir,
                InstallOptions {
                    yes: cli.yes,
                    non_interactive: cli.non_interactive,
                    force: cli.force,
                },
            )
        }
    }
}

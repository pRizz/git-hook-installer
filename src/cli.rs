//! Command-line interface definitions and argument parsing.
//!
//! This module defines the CLI structure, commands, and options using clap.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "git-hook-installer", version, about)]
pub struct Cli {
    /// Automatically answer "yes" to prompts
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Do not prompt; fail instead of asking questions
    #[arg(long)]
    pub non_interactive: bool,

    /// Overwrite existing hook files without prompting
    #[arg(short = 'f', long)]
    pub force: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install or update a premade hook into the current repository
    Install {
        /// Hook to install
        #[arg(value_enum)]
        hook: Option<HookKind>,

        /// Directory containing the Cargo.toml to use (only used for pre-commit)
        #[arg(long, value_name = "DIR")]
        manifest_dir: Option<PathBuf>,
    },
    /// Install/update a hook across many git repos under a directory
    ///
    /// This is useful when you have a parent folder containing many repositories.
    /// By default, the scan root is the current directory.
    InstallRecursive {
        /// Hook to install
        #[arg(value_enum)]
        hook: Option<HookKind>,

        /// Directory containing the Cargo.toml to use (only used for pre-commit)
        #[arg(long, value_name = "DIR")]
        manifest_dir: Option<PathBuf>,

        /// Directory to scan for git repos (defaults to current directory)
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Disable the managed `pre-commit` hook block installed by git-hook-installer
    Disable,
    /// Uninstall the managed `pre-commit` hook block installed by git-hook-installer
    Uninstall,
    /// List available premade hooks
    List,
    /// Inspect and report current hook state for this repository
    Status {
        /// Print more details (e.g. hook contents summary)
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HookKind {
    /// pre-commit hook that runs common formatters/linters (managed block)
    PreCommit,
}

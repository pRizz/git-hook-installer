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

        /// Scan for git repos under a directory instead of operating on the current repo
        ///
        /// When enabled (or when `--dir/--max-depth` are used), the command scans `--dir`
        /// (or the current directory if omitted) up to `--max-depth` and runs in each repo found.
        #[arg(long)]
        recursive: bool,

        /// How deep to scan for git repositories when in scan mode (default: 0)
        ///
        /// Depth 0 scans only the scan-root directory itself.
        /// Depth 1 scans the scan-root and its immediate children.
        ///
        /// Note: if `--recursive` is provided and `--max-depth` is omitted, the effective default is 1.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Directory to scan for git repos when in scan mode (defaults to current directory)
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Disable the managed `pre-commit` hook block installed by git-hook-installer
    Disable {
        /// Scan for git repos under a directory instead of operating on the current repo
        ///
        /// When enabled (or when `--dir/--max-depth` are used), the command scans `--dir`
        /// (or the current directory if omitted) up to `--max-depth` and runs in each repo found.
        #[arg(long)]
        recursive: bool,

        /// How deep to scan for git repositories when in scan mode (default: 0)
        ///
        /// Depth 0 scans only the scan-root directory itself.
        /// Depth 1 scans the scan-root and its immediate children.
        ///
        /// Note: if `--recursive` is provided and `--max-depth` is omitted, the effective default is 1.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Directory to scan for git repos when in scan mode (defaults to current directory)
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Uninstall the managed `pre-commit` hook block installed by git-hook-installer
    Uninstall {
        /// Scan for git repos under a directory instead of operating on the current repo
        ///
        /// When enabled (or when `--dir/--max-depth` are used), the command scans `--dir`
        /// (or the current directory if omitted) up to `--max-depth` and runs in each repo found.
        #[arg(long)]
        recursive: bool,

        /// How deep to scan for git repositories when in scan mode (default: 0)
        ///
        /// Depth 0 scans only the scan-root directory itself.
        /// Depth 1 scans the scan-root and its immediate children.
        ///
        /// Note: if `--recursive` is provided and `--max-depth` is omitted, the effective default is 1.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Directory to scan for git repos when in scan mode (defaults to current directory)
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// List available premade hooks
    List,
    /// Inspect and report current hook state for this repository
    Status {
        /// Print more details (e.g. hook contents summary)
        #[arg(long)]
        verbose: bool,

        /// Scan for git repos under a directory instead of operating on the current repo
        ///
        /// When enabled (or when `--dir/--max-depth` are used), the command scans `--dir`
        /// (or the current directory if omitted) up to `--max-depth` and runs in each repo found.
        #[arg(long)]
        recursive: bool,

        /// How deep to scan for git repositories when in scan mode (default: 0)
        ///
        /// Depth 0 scans only the scan-root directory itself.
        /// Depth 1 scans the scan-root and its immediate children.
        ///
        /// Note: if `--recursive` is provided and `--max-depth` is omitted, the effective default is 1.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Directory to scan for git repos when in scan mode (defaults to current directory)
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HookKind {
    /// pre-commit hook that runs common formatters/linters (managed block)
    PreCommit,
}

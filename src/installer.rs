use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dialoguer::Confirm;

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::cli::HookKind;
use crate::hooks::{cargo_fmt_pre_commit_script, install_hook_script, InstallOptions};

#[derive(Debug, Clone)]
pub enum ResolvedHook {
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
    let hook = maybe_hook.unwrap_or(HookKind::CargoFmtPreCommit);

    match hook {
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
    options: InstallOptions,
) -> Result<()> {
    match kind {
        ResolvedHook::CargoFmtPreCommit { cargo_dir } => {
            let script = cargo_fmt_pre_commit_script(&cargo_dir);
            install_hook_script(git_dir, "pre-commit", &script, options)
        }
    }
}

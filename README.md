# git-hook-installer

A small Rust CLI that installs premade git hooks into the **current** repository.

[![crates.io](https://img.shields.io/crates/v/git-hook-installer.svg)](https://crates.io/crates/git-hook-installer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Repository](https://img.shields.io/badge/repo-github-blue)](https://github.com/pRizz/git-hook-installer)

## Install

From [crates.io](https://crates.io/crates/git-hook-installer):

```bash
cargo install git-hook-installer
```

Or from source (this repo):

```bash
cargo install --path .
```

## Usage

Run with no arguments to offer to install/update the managed `pre-commit` hook:

```bash
git-hook-installer
```

Inspect the current hook state:

```bash
git-hook-installer status
```

Install/update the managed `pre-commit` hook directly:

```bash
git-hook-installer install pre-commit
```

Disable the managed `pre-commit` block (without removing it):

```bash
git-hook-installer disable
```

Uninstall the managed `pre-commit` block (preserves any other pre-commit logic you already had):

```bash
git-hook-installer uninstall
```

Install the legacy standalone `cargo fmt` pre-commit hook directly:

```bash
git-hook-installer install cargo-fmt-pre-commit
```

If your repo has multiple `Cargo.toml` files (monorepo), pick which one the hook should use:

```bash
git-hook-installer install pre-commit --manifest-dir crates/my-crate
```

## Behavior

- **git repo detection**: walks up parent directories looking for `.git` (supports worktrees where `.git` is a file).
- **safe overwrites**: if a hook already exists, it will prompt before backing it up (or use `--force` / `--yes`).
- **hook installed**: `.git/hooks/pre-commit` contains a **managed block** (marked with `git-hook-installer` begin/end markers) which can run a set of formatters/linters and **re-stage** changes.
- **no repo config**: all settings are stored **inside the hook file in `.git/hooks/`** (nothing is written to your repository).
- **auto-fix safety**:
  - If you have **unstaged/untracked** changes, the hook stashes them with `git stash push --keep-index --include-untracked`, runs auto-fix on the staged files, re-stages, and then restores the stash.
  - If a formatting step errors, the hook attempts a **best-effort rollback** (reset + re-apply saved staged diff, plus stash restore if used).
- **snapshots before edits**: before `git-hook-installer` modifies `.git/hooks/pre-commit`, it snapshots the current file to `.git/hooks/pre-commit.snapshot-YYYY-MM-DD-HH-MM-SS` and keeps the newest **10** snapshots by default.

## Options

- `-y, --yes`: auto-confirm prompts
- `--non-interactive`: never prompt (fails on ambiguity or existing hooks unless `--force`)
- `-f, --force`: overwrite existing hook (backs it up first)

## Links

- [Repository](https://github.com/pRizz/git-hook-installer)
- [Crates.io](https://crates.io/crates/git-hook-installer)
- [Documentation](https://docs.rs/git-hook-installer)

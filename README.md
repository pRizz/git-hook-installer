# git-hook-installer

A small Rust CLI that installs premade git hooks into the **current** repository.

## Install

From source (this repo):

```bash
cargo install --path .
```

## Usage

Run with no arguments to auto-detect a Cargo/Rust repo and offer to install a `pre-commit` hook:

```bash
git-hook-installer
```

Inspect the current hook state:

```bash
git-hook-installer status
```

Install the `cargo fmt` pre-commit hook directly:

```bash
git-hook-installer install cargo-fmt-pre-commit
```

If your repo has multiple `Cargo.toml` files (monorepo), pick which one the hook should use:

```bash
git-hook-installer install cargo-fmt-pre-commit --manifest-dir crates/my-crate
```

## Behavior

- **git repo detection**: walks up parent directories looking for `.git` (supports worktrees where `.git` is a file).
- **safe overwrites**: if a hook already exists, it will prompt before backing it up (or use `--force` / `--yes`).
- **hook installed**: `.git/hooks/pre-commit` runs `cargo fmt` to format code before committing (does not block commits).

## Options

- `-y, --yes`: auto-confirm prompts
- `--non-interactive`: never prompt (fails on ambiguity or existing hooks unless `--force`)
- `-f, --force`: overwrite existing hook (backs it up first)

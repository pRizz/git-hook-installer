# git-hook-installer

A small Rust CLI that installs a **managed git hook block** (currently `pre-commit`) with sensible defaults — **without adding any config files or DSLs to your repository**.

[![crates.io](https://img.shields.io/crates/v/git-hook-installer.svg)](https://crates.io/crates/git-hook-installer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Repository](https://img.shields.io/badge/repo-github-blue)](https://github.com/pRizz/git-hook-installer)

## Why this tool?

There are lots of ways to manage git hooks. `git-hook-installer` is intentionally opinionated:

- **No repo config / no new DSL**: nothing is written into your repo (no YAML/TOML config file, no “hook language” to learn). Settings live *inside the managed block* in `.git/hooks/`.
- **Plays nicely with existing hooks**: it only adds/removes a clearly-marked managed block; it won’t clobber unrelated hook logic.
- **Reasonable defaults (and avoids surprises)**: language/tooling sections are included only when there’s positive “proof” the repo uses that language.
- **Safe and reversible**: snapshots are created before edits, and the hook uses safe stash/restore + best-effort rollback around auto-fix steps.
- **Works for monorepos and many repos**: you can opt into scan mode (`--recursive`) to operate across many repositories under a directory.

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

### Scan mode (opt-in) for bulk operations

Most subcommands operate on the **current repository** (by walking up parent directories to find `.git`).

If you want to operate across many repositories under a directory, enable **scan mode** using
`--recursive` (or by providing `--dir` / `--max-depth`).

- **scan root**: `--dir DIR` (defaults to current directory)
- **scan depth**: `--max-depth N`
  - If `--recursive` is provided and `--max-depth` is omitted, the effective default is **1**
  - Otherwise (e.g. if scan mode is triggered by `--dir` alone), the effective default is **0**
    - Depth **0** scans **only the scan-root directory itself**
    - Depth **1** scans the scan-root and its immediate children

In scan mode, mutating operations (`install`, `disable`, `uninstall`) show a repo preview and ask for
confirmation unless `--yes` is used.

Recursively install/update the managed `pre-commit` hook across many repos under a directory:

```bash
# scans current directory (default depth is 1 when --recursive is present)
git-hook-installer install --recursive

# scan a directory
git-hook-installer install --recursive --dir ~/src

# increase scan depth
git-hook-installer install --recursive --dir ~/src --max-depth 3

# skip the confirmation prompt
git-hook-installer --yes install --recursive --dir ~/src --max-depth 3
```

Recursively uninstall the managed `pre-commit` hook block across many repos under a directory:

```bash
git-hook-installer uninstall --recursive --dir ~/src --max-depth 3
```

Recursively disable the managed `pre-commit` hook block across many repos under a directory:

```bash
git-hook-installer disable --recursive --dir ~/src --max-depth 3
```

Recursively inspect the current hook state across many repos under a directory:

```bash
git-hook-installer status --recursive --dir ~/src --max-depth 3

# show more details per repo
git-hook-installer status --recursive --dir ~/src --max-depth 3 --verbose
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

If your repo has multiple `Cargo.toml` files (monorepo), pick which one the hook should use:

```bash
git-hook-installer install pre-commit --manifest-dir crates/my-crate
```

## Behavior

- **git repo detection**: walks up parent directories looking for `.git` (supports worktrees where `.git` is a file).
- **scan mode**: `install|disable|uninstall|status --recursive [--dir DIR] [--max-depth N]` scans for git repos under a directory and runs the command in each repo. If `--recursive` is provided and `--max-depth` is omitted, the effective default depth is **1**.
- **safe overwrites**: if a hook already exists, it will prompt before backing it up (or use `--force` / `--yes`).
- **hook installed**: `.git/hooks/pre-commit` contains a **managed block** (marked with `git-hook-installer` begin/end markers) which can run a set of formatters/linters and **re-stage** changes.
- **no repo config**: all settings are stored **inside the hook file in `.git/hooks/`** (nothing is written to your repository).
- **proof-based language enabling (to avoid surprises)**:
  - At install time, `git-hook-installer` tries to determine which languages your repo actually uses.
  - The generated hook only includes language sections when there is **positive evidence** (“proof”) that the repo uses that language. This prevents, for example, adding JS/TS lints to a non-JS repo.
  - If you later add a new language to the repo (or add config files), re-run `git-hook-installer install pre-commit` to re-detect and regenerate the hook.
  - Monorepos are supported: detection includes a **bounded shallow scan** so nested packages can still be detected without doing an expensive full repo walk.
- **toolchain auto-selection**:
  - For languages that are enabled (proven), the installer auto-selects the most likely toolchain (e.g. Biome vs Prettier+ESLint, Ruff vs Black, Spotless vs ktlint) based on common config signals.
  - In interactive installs it prints a short “auto-selected/defaulting” summary; in `--non-interactive` mode it stays quiet.
- **what counts as “proof”** (high-level):
  - **JS/TS**: `package.json` / lockfiles / `tsconfig.json` / `jsconfig.json` / Biome / ESLint / Prettier config, or a shallow scan that finds JS/TS source files.
    - Note: Prettier-based formatting for **Markdown/YAML** is tied to JS/TS being enabled (since it uses the same toolchain).
    - If the repo is detected as **TypeScript**, the hook also runs a **`tsc --noEmit` typecheck** when staged changes include `*.ts/*.tsx` or `tsconfig.json` (uses `tsc` on PATH or `npx --yes tsc`).
  - **Python**: `pyproject.toml`, requirements/setup files, common lockfiles, or a shallow scan that finds `.py` files.
  - **Java/Kotlin**: Gradle/Maven files, or a shallow scan that finds `.java/.kt/.kts` files.
  - **Go**: `go.mod/go.work/go.sum`, or a shallow scan that finds `.go` files.
  - **Ruby**: `Gemfile` / `.ruby-version` / `Rakefile`, or a shallow scan that finds `.rb` files.
  - **Shell**: `.shellcheckrc` / `.shfmt`, or a shallow scan that finds shell scripts.
  - **Terraform**: `.terraform.lock.hcl`, or a shallow scan that finds `.tf/.tfvars` files.
  - **C/C++**: `.clang-format`, or a shallow scan that finds common C/C++ file extensions.
  - **Rust**: `cargo fmt` only runs when a Cargo manifest directory was resolved (or passed via `--manifest-dir`).
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

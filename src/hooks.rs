//! Git hook installation and script generation.
//!
//! This module handles writing hook scripts to the git hooks directory,
//! including backup of existing hooks, permission management, and generation
//! of hook script content (e.g., cargo-fmt pre-commit hooks).

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use dialoguer::Confirm;
use time::{format_description, OffsetDateTime};

#[derive(Clone, Copy)]
pub struct InstallOptions {
    pub yes: bool,
    pub non_interactive: bool,
    pub force: bool,
}

pub const PRE_COMMIT_HOOK_NAME: &str = "pre-commit";
const MANAGED_BLOCK_BEGIN: &str = "# >>> git-hook-installer managed block >>>";
const MANAGED_BLOCK_END: &str = "# <<< git-hook-installer managed block <<<";
const DEFAULT_MAX_SNAPSHOTS: usize = 10;

pub fn install_hook_script(
    git_dir: &Path,
    hook_name: &str,
    hook_contents: &str,
    options: InstallOptions,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let hook_path = hooks_dir.join(hook_name);
    write_hook_file(&hook_path, hook_contents.as_bytes(), options)?;

    println!("Installed `{}` hook at {}", hook_name, hook_path.display());
    Ok(())
}

pub fn cargo_fmt_pre_commit_script(cargo_dir: &Path) -> String {
    format!(
        r#"#!/bin/sh
set -e

cd "{}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found; skipping cargo fmt"
  exit 0
fi

echo "Running cargo fmt..."
cargo fmt

"#,
        shell_escape_path(cargo_dir)
    )
}

#[derive(Debug, Clone, Copy)]
pub enum JsTsTool {
    Biome,
    PrettierEslint,
}

#[derive(Debug, Clone, Copy)]
pub enum PythonTool {
    Ruff,
    Black,
}

#[derive(Debug, Clone, Copy)]
pub enum JavaKotlinTool {
    Spotless,
    Ktlint,
}

#[derive(Debug, Clone)]
pub struct ManagedPreCommitSettings {
    pub enabled: bool,
    pub js_ts_tool: JsTsTool,
    pub python_tool: PythonTool,
    pub java_kotlin_tool: JavaKotlinTool,
    /// If set, `cargo fmt` will run from this directory.
    pub maybe_cargo_manifest_dir: Option<std::path::PathBuf>,
}

pub fn managed_pre_commit_block(settings: &ManagedPreCommitSettings, repo_root: &Path) -> String {
    let js_ts_tool = match settings.js_ts_tool {
        JsTsTool::Biome => "biome",
        JsTsTool::PrettierEslint => "prettier+eslint",
    };

    let python_tool = match settings.python_tool {
        PythonTool::Ruff => "ruff",
        PythonTool::Black => "black",
    };

    let java_kotlin_tool = match settings.java_kotlin_tool {
        JavaKotlinTool::Spotless => "spotless",
        JavaKotlinTool::Ktlint => "ktlint",
    };

    let cargo_manifest_dir_note = settings
        .maybe_cargo_manifest_dir
        .as_deref()
        .map(|dir| crate::util::relative_display(repo_root, dir))
        .unwrap_or_else(|| "(none)".to_string());

    let cargo_manifest_dir_for_shell = settings
        .maybe_cargo_manifest_dir
        .as_deref()
        .map(shell_escape_path)
        .unwrap_or_else(|| "(none)".to_string());

    let enabled = if settings.enabled { "1" } else { "0" };

    // NOTE: This must remain POSIX-sh compatible.
    format!(
        r#"{MANAGED_BLOCK_BEGIN}
# git-hook-installer settings (stored locally in this hook file):
#   enabled={enabled}
#   js_ts_tool={js_ts_tool}
#   python_tool={python_tool}
#   java_kotlin_tool={java_kotlin_tool}
#   cargo_manifest_dir={cargo_manifest_dir_note}
#   default_mode=fix
#   unstaged_changes=stash(--keep-index --include-untracked) + restore
#   rollback_on_error=git reset --hard + re-apply saved index diff (+ stash pop if used)

GHI_ENABLED={enabled}
GHI_JS_TS_TOOL="{js_ts_tool}"
GHI_PYTHON_TOOL="{python_tool}"
GHI_JAVA_KOTLIN_TOOL="{java_kotlin_tool}"
GHI_CARGO_MANIFEST_DIR="{cargo_manifest_dir_for_shell}"

ghi_echo() {{
  printf '%s\n' "git-hook-installer: $*"
}}

ghi_has_cmd() {{
  command -v "$1" >/dev/null 2>&1
}}

ghi_staged_files() {{
  git diff --cached --name-only --diff-filter=ACMR
}}

ghi_filter_by_ext() {{
  # usage: ghi_filter_by_ext "<files>" "<pattern1>" "<pattern2>" ...
  files="$1"
  shift
  if [ -z "$files" ]; then
    return 0
  fi

  for file in $files; do
    for pattern in "$@"; do
      case "$file" in
        $pattern)
          printf '%s\n' "$file"
          break
          ;;
      esac
    done
  done
}}

ghi_git_add_list() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  # Avoid xargs -0 portability issues; newline-in-filenames is extremely uncommon.
  for file in $files; do
    git add -- "$file"
  done
}}

ghi_make_tmpdir() {{
  # mktemp(1) has slightly different flags across platforms.
  tmp="$(mktemp -d 2>/dev/null || mktemp -d -t ghi)"
  printf '%s' "$tmp"
}}

ghi_has_unstaged_or_untracked() {{
  if ! git diff --quiet; then
    return 0
  fi
  if [ -n "$(git ls-files --others --exclude-standard)" ]; then
    return 0
  fi
  return 1
}}

GHI_TMPDIR=""
GHI_DID_STASH=0
GHI_SUCCESS=0

ghi_rollback() {{
  # Best-effort: restore to state from start of hook run.
  ghi_echo "Rolling back index/worktree to pre-hook state..."

  # Reset index and worktree to HEAD.
  git reset --hard >/dev/null 2>&1 || true

  if [ -s "$GHI_TMPDIR/index.patch" ]; then
    git apply --index "$GHI_TMPDIR/index.patch" >/dev/null 2>&1 || true
  fi

  if [ "$GHI_DID_STASH" = "1" ]; then
    # Pop the stash we created (expected to be top-of-stack).
    git stash pop --index >/dev/null 2>&1 || {{
      ghi_echo "WARNING: stash pop had conflicts; your stash was preserved. Run: git stash list"
      return 0
    }}
  else
    if [ -s "$GHI_TMPDIR/worktree.patch" ]; then
      git apply "$GHI_TMPDIR/worktree.patch" >/dev/null 2>&1 || true
    fi
  fi
}}

ghi_cleanup() {{
  status="$1"

  if [ "$status" -ne 0 ] && [ "$GHI_SUCCESS" -ne 1 ]; then
    ghi_rollback
  fi

  if [ "$status" -eq 0 ] && [ "$GHI_DID_STASH" = "1" ]; then
    # Restore unstaged/untracked changes after successful formatting.
    git stash pop --index >/dev/null 2>&1 || {{
      ghi_echo "WARNING: stash pop had conflicts; your stash was preserved. Run: git stash list"
      return 0
    }}
  fi

  if [ -n "$GHI_TMPDIR" ] && [ -d "$GHI_TMPDIR" ]; then
    rm -rf "$GHI_TMPDIR" >/dev/null 2>&1 || true
  fi
}}

ghi_run_js_ts_biome() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ghi_has_cmd biome; then
    ghi_echo "Running biome (fix + lint)..."
    biome check --write $files
    return 0
  fi

  if ghi_has_cmd npx; then
    ghi_echo "Running biome via npx (fix + lint)..."
    npx --no-install biome check --write $files
    return 0
  fi

  ghi_echo "biome not found; skipping JS/TS"
  return 0
}}

ghi_run_js_ts_prettier_eslint() {{
  files_js_ts_json="$1"
  files_js_ts="$2"

  if [ -n "$files_js_ts_json" ]; then
    if ghi_has_cmd prettier; then
      ghi_echo "Running prettier (fix)..."
      prettier --write $files_js_ts_json
    elif ghi_has_cmd npx; then
      ghi_echo "Running prettier via npx (fix)..."
      npx --no-install prettier --write $files_js_ts_json
    else
      ghi_echo "prettier not found; skipping prettier"
    fi
  fi

  if [ -n "$files_js_ts" ]; then
    if ghi_has_cmd eslint; then
      ghi_echo "Running eslint (fix)..."
      eslint --fix $files_js_ts
    elif ghi_has_cmd npx; then
      ghi_echo "Running eslint via npx (fix)..."
      npx --no-install eslint --fix $files_js_ts
    else
      ghi_echo "eslint not found; skipping eslint"
    fi
  fi
}}

ghi_run_python_ruff() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd ruff; then
    ghi_echo "ruff not found; skipping Python"
    return 0
  fi

  ghi_echo "Running ruff format (fix)..."
  ruff format $files

  ghi_echo "Running ruff check --fix..."
  ruff check --fix $files
}}

ghi_run_python_black() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd black; then
    ghi_echo "black not found; skipping Python"
    return 0
  fi

  ghi_echo "Running black (fix)..."
  black $files
}}

ghi_run_go() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd gofmt; then
    ghi_echo "gofmt not found; skipping Go"
    return 0
  fi

  ghi_echo "Running gofmt (fix)..."
  gofmt -w $files
}}

ghi_run_shell() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ghi_has_cmd shfmt; then
    ghi_echo "Running shfmt (fix)..."
    shfmt -w $files
  else
    ghi_echo "shfmt not found; skipping shell formatting"
  fi

  if ghi_has_cmd shellcheck; then
    ghi_echo "Running shellcheck (lint)..."
    shellcheck $files
  else
    ghi_echo "shellcheck not found; skipping shellcheck"
  fi
}}

ghi_run_terraform() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd terraform; then
    ghi_echo "terraform not found; skipping Terraform"
    return 0
  fi

  dirs="$(printf '%s\n' $files | while read -r f; do dirname "$f"; done | sort -u)"
  if [ -z "$dirs" ]; then
    return 0
  fi

  for d in $dirs; do
    ghi_echo "Running terraform fmt in $d..."
    (cd "$d" && terraform fmt)
  done
}}

ghi_run_clang_format() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd clang-format; then
    ghi_echo "clang-format not found; skipping C/C++"
    return 0
  fi

  ghi_echo "Running clang-format (fix)..."
  clang-format -i $files
}}

ghi_run_java_kotlin_spotless() {{
  all_staged_files="$1"
  if [ -z "$all_staged_files" ]; then
    return 0
  fi

  if [ -x "./gradlew" ]; then
    ghi_echo "Running ./gradlew spotlessApply (fix)..."
    ./gradlew -q spotlessApply
    ghi_git_add_list "$all_staged_files"
    return 0
  fi

  if ghi_has_cmd gradle; then
    ghi_echo "Running gradle spotlessApply (fix)..."
    gradle -q spotlessApply
    ghi_git_add_list "$all_staged_files"
    return 0
  fi

  ghi_echo "spotless requested but gradle/gradlew not found; skipping"
  return 0
}}

ghi_run_java_kotlin_ktlint() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd ktlint; then
    ghi_echo "ktlint not found; skipping Kotlin"
    return 0
  fi

  ghi_echo "Running ktlint -F (fix)..."
  ktlint -F $files
}}

ghi_run_rubocop() {{
  files="$1"
  if [ -z "$files" ]; then
    return 0
  fi

  if ! ghi_has_cmd rubocop; then
    ghi_echo "rubocop not found; skipping Ruby"
    return 0
  fi

  ghi_echo "Running rubocop -A (fix)..."
  rubocop -A $files
}}

ghi_run_cargo_fmt() {{
  if [ "$GHI_CARGO_MANIFEST_DIR" = "(none)" ]; then
    return 0
  fi

  if ! ghi_has_cmd cargo; then
    ghi_echo "cargo not found; skipping cargo fmt"
    return 0
  fi

  # NOTE: cargo fmt formats the workspace configured by this manifest dir.
  ghi_echo "Running cargo fmt..."
  cd "$GHI_CARGO_MANIFEST_DIR"
  cargo fmt
}}

ghi_main() {{
  if [ "$GHI_ENABLED" != "1" ]; then
    return 0
  fi

  set -eu

  if ! ghi_has_cmd git; then
    ghi_echo "git not found; skipping"
    return 0
  fi

  GHI_TMPDIR="$(ghi_make_tmpdir)"
  git diff --cached --binary > "$GHI_TMPDIR/index.patch" 2>/dev/null || true
  git diff --binary > "$GHI_TMPDIR/worktree.patch" 2>/dev/null || true

  if ghi_has_unstaged_or_untracked; then
    ghi_echo "Stashing unstaged/untracked changes (keeping index) before auto-fix..."
    git stash push --keep-index --include-untracked -m "git-hook-installer pre-commit auto-stash" >/dev/null 2>&1
    GHI_DID_STASH=1
  fi

  staged="$(ghi_staged_files)"
  if [ -z "$staged" ]; then
    return 0
  fi

  # Filter file lists.
  files_js_ts="$(ghi_filter_by_ext "$staged" "*.js" "*.jsx" "*.ts" "*.tsx")"
  files_js_ts_json="$(ghi_filter_by_ext "$staged" "*.js" "*.jsx" "*.ts" "*.tsx" "*.json")"
  files_md_yaml="$(ghi_filter_by_ext "$staged" "*.md" "*.markdown" "*.yml" "*.yaml")"
  files_py="$(ghi_filter_by_ext "$staged" "*.py")"
  files_go="$(ghi_filter_by_ext "$staged" "*.go")"
  files_sh="$(ghi_filter_by_ext "$staged" "*.sh" "*.bash" "*.zsh")"
  files_tf="$(ghi_filter_by_ext "$staged" "*.tf" "*.tfvars")"
  files_c_cpp="$(ghi_filter_by_ext "$staged" "*.c" "*.cc" "*.cpp" "*.cxx" "*.h" "*.hh" "*.hpp" "*.hxx")"
  files_kt="$(ghi_filter_by_ext "$staged" "*.kt" "*.kts")"
  files_rb="$(ghi_filter_by_ext "$staged" "*.rb")"

  # JS/TS + JSON
  if [ "$GHI_JS_TS_TOOL" = "biome" ]; then
    ghi_run_js_ts_biome "$files_js_ts_json"
  else
    ghi_run_js_ts_prettier_eslint "$files_js_ts_json" "$files_js_ts"
  fi
  ghi_git_add_list "$files_js_ts_json"

  # Markdown/YAML always uses prettier if available.
  if [ -n "$files_md_yaml" ]; then
    if ghi_has_cmd prettier; then
      ghi_echo "Running prettier on Markdown/YAML (fix)..."
      prettier --write $files_md_yaml
      ghi_git_add_list "$files_md_yaml"
    elif ghi_has_cmd npx; then
      ghi_echo "Running prettier via npx on Markdown/YAML (fix)..."
      npx --no-install prettier --write $files_md_yaml
      ghi_git_add_list "$files_md_yaml"
    else
      ghi_echo "prettier not found; skipping Markdown/YAML formatting"
    fi
  fi

  # Python
  if [ "$GHI_PYTHON_TOOL" = "ruff" ]; then
    ghi_run_python_ruff "$files_py"
  else
    ghi_run_python_black "$files_py"
  fi
  ghi_git_add_list "$files_py"

  # Go
  ghi_run_go "$files_go"
  ghi_git_add_list "$files_go"

  # Shell
  ghi_run_shell "$files_sh"
  ghi_git_add_list "$files_sh"

  # Terraform
  ghi_run_terraform "$files_tf"
  ghi_git_add_list "$files_tf"

  # C/C++
  ghi_run_clang_format "$files_c_cpp"
  ghi_git_add_list "$files_c_cpp"

  # Java/Kotlin
  if [ "$GHI_JAVA_KOTLIN_TOOL" = "spotless" ]; then
    ghi_run_java_kotlin_spotless "$staged"
  else
    ghi_run_java_kotlin_ktlint "$files_kt"
    ghi_git_add_list "$files_kt"
  fi

  # Ruby
  ghi_run_rubocop "$files_rb"
  ghi_git_add_list "$files_rb"

  # Rust
  # Note: cargo fmt formats at the workspace level and may touch files beyond staging.
  ghi_run_cargo_fmt

  GHI_SUCCESS=1
  return 0
}}

trap 'ghi_cleanup $?' EXIT HUP INT TERM
ghi_main
{MANAGED_BLOCK_END}
"#
    )
}

pub fn upsert_managed_pre_commit_hook(
    git_dir: &Path,
    block: &str,
    options: InstallOptions,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let hook_path = hooks_dir.join(PRE_COMMIT_HOOK_NAME);
    upsert_managed_block_in_file(&hook_path, block, options)?;
    set_executable(&hook_path)
        .with_context(|| format!("Failed to mark {} as executable", hook_path.display()))?;
    println!(
        "Installed `{}` hook at {}",
        PRE_COMMIT_HOOK_NAME,
        hook_path.display()
    );
    Ok(())
}

pub fn disable_managed_pre_commit_hook(git_dir: &Path) -> Result<()> {
    let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
    if !hook_path.exists() {
        return Err(anyhow!("No pre-commit hook exists at {}", hook_path.display()));
    }

    let contents = fs::read_to_string(&hook_path)
        .with_context(|| format!("Failed to read {}", hook_path.display()))?;
    let updated = disable_managed_block(&contents)?;
    write_hook_with_snapshot_if_changed(&hook_path, &contents, &updated)?;
    println!("Disabled managed git-hook-installer block in {}", hook_path.display());
    Ok(())
}

pub fn uninstall_managed_pre_commit_hook(git_dir: &Path) -> Result<()> {
    let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
    if !hook_path.exists() {
        return Err(anyhow!("No pre-commit hook exists at {}", hook_path.display()));
    }

    let contents = fs::read_to_string(&hook_path)
        .with_context(|| format!("Failed to read {}", hook_path.display()))?;
    let updated = uninstall_managed_block(&contents)?;

    if updated.trim().is_empty() {
        create_hook_snapshot_and_prune(&hook_path, DEFAULT_MAX_SNAPSHOTS)?;
        fs::remove_file(&hook_path)
            .with_context(|| format!("Failed to remove {}", hook_path.display()))?;
        println!("Removed {}", hook_path.display());
        return Ok(());
    }

    write_hook_with_snapshot_if_changed(&hook_path, &contents, &updated)?;
    println!("Uninstalled managed git-hook-installer block in {}", hook_path.display());
    Ok(())
}

fn shell_escape_path(path: &Path) -> String {
    // Minimal escaping for POSIX sh: wrap in double quotes and escape embedded quotes/backslashes,
    // dollar signs, and backticks to prevent command injection.
    let raw = path.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len() + 2);
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '`' => escaped.push_str("\\`"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn write_hook_file(path: &Path, contents: &[u8], options: InstallOptions) -> Result<()> {
    if path.exists() {
        handle_existing_hook(path, options)?;
    }

    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create hook file at {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("Failed to write hook file at {}", path.display()))?;

    set_executable(path)
        .with_context(|| format!("Failed to mark {} as executable", path.display()))?;
    Ok(())
}

fn handle_existing_hook(path: &Path, options: InstallOptions) -> Result<()> {
    if options.force || options.yes {
        return backup_existing_hook(path);
    }

    if options.non_interactive {
        return Err(anyhow!(
            "Hook already exists at {} (use --force to overwrite)",
            path.display()
        ));
    }

    println!("Hook already exists at {}.", path.display());
    let should_overwrite = Confirm::new()
        .with_prompt("Back up existing hook and overwrite?")
        .default(false)
        .interact()
        .context("Failed to read confirmation from stdin")?;

    if !should_overwrite {
        return Err(anyhow!("Aborted (existing hook was not modified)."));
    }

    backup_existing_hook(path)
}

fn backup_existing_hook(path: &Path) -> Result<()> {
    let maybe_file_name = path.file_name().and_then(OsStr::to_str);
    let Some(file_name) = maybe_file_name else {
        return Err(anyhow!("Invalid hook path: {}", path.display()));
    };

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid hook path (no parent): {}", path.display()))?;

    let mut counter: u32 = 0;
    loop {
        let suffix = if counter == 0 {
            ".bak".to_string()
        } else {
            format!(".bak.{}", counter)
        };

        let backup_path = parent.join(format!("{}{}", file_name, suffix));
        if backup_path.exists() {
            counter = counter.saturating_add(1);
            if counter > 10_000 {
                return Err(anyhow!(
                    "Too many backup files exist for {}",
                    path.display()
                ));
            }
            continue;
        }

        fs::copy(path, &backup_path).with_context(|| {
            format!(
                "Failed to back up existing hook from {} to {}",
                path.display(),
                backup_path.display()
            )
        })?;
        println!("Backed up existing hook to {}", backup_path.display());
        return Ok(());
    }
}

fn upsert_managed_block_in_file(path: &Path, block: &str, options: InstallOptions) -> Result<()> {
    let existing = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read hook file at {}", path.display()))?;
        Some(contents)
    } else {
        None
    };

    let updated = match existing.as_deref() {
        None => ensure_shebang(block),
        Some(contents) => {
            let has_managed = contents.contains(MANAGED_BLOCK_BEGIN) && contents.contains(MANAGED_BLOCK_END);
            if !has_managed {
                // This is an existing user hook; get consent and back it up before modifying.
                handle_existing_hook(path, options)?;
            }
            upsert_managed_block(contents, block)
        }
    };

    if let Some(existing) = existing.as_deref() {
        if existing == updated {
            // No changes to write; do not create a snapshot.
            return Ok(());
        }
        create_hook_snapshot_and_prune(path, DEFAULT_MAX_SNAPSHOTS)?;
    }

    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create hook file at {}", path.display()))?;
    file.write_all(updated.as_bytes())
        .with_context(|| format!("Failed to write hook file at {}", path.display()))?;
    Ok(())
}

fn ensure_shebang(contents: &str) -> String {
    let first_line = contents.lines().next().unwrap_or_default();
    if first_line.starts_with("#!") {
        return contents.to_string();
    }
    format!("#!/bin/sh\n{contents}")
}

fn upsert_managed_block(existing: &str, block: &str) -> String {
    let mut lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let block_lines: Vec<&str> = block.lines().collect();

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start <= end => {
            lines.splice(start..=end, block_lines);
            ensure_shebang(&normalize_newline_join(&lines))
        }
        _ => {
            // No managed block: insert after shebang if present.
            let insert_at = if !lines.is_empty() && lines[0].starts_with("#!") {
                1
            } else {
                0
            };

            let mut out: Vec<&str> = Vec::with_capacity(lines.len() + block_lines.len() + 2);
            out.extend_from_slice(&lines[..insert_at]);
            if insert_at > 0 {
                out.push("");
            }
            out.extend_from_slice(&block_lines);
            out.push("");
            out.extend_from_slice(&lines[insert_at..]);
            ensure_shebang(&normalize_newline_join(&out))
        }
    }
}

fn uninstall_managed_block(existing: &str) -> Result<String> {
    let lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let (Some(start), Some(end)) = (start_idx, end_idx) else {
        return Err(anyhow!("No managed git-hook-installer block found in pre-commit hook"));
    };
    if start > end {
        return Err(anyhow!("Invalid managed block markers in pre-commit hook"));
    }

    let mut out = lines;
    out.splice(start..=end, []);
    Ok(normalize_newline_join(&out))
}

fn disable_managed_block(existing: &str) -> Result<String> {
    let lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let (Some(start), Some(end)) = (start_idx, end_idx) else {
        return Err(anyhow!("No managed git-hook-installer block found in pre-commit hook"));
    };
    if start > end {
        return Err(anyhow!("Invalid managed block markers in pre-commit hook"));
    }

    let mut did_change = false;
    let mut out = Vec::with_capacity(lines.len());

    for (idx, line) in lines.iter().enumerate() {
        if idx < start || idx > end {
            out.push(*line);
            continue;
        }

        if line.trim_start().starts_with("GHI_ENABLED=") {
            out.push("GHI_ENABLED=0");
            did_change = true;
            continue;
        }

        out.push(*line);
    }

    if !did_change {
        return Err(anyhow!("Managed block found, but no GHI_ENABLED setting line was found"));
    }

    Ok(normalize_newline_join(&out))
}

fn normalize_newline_join(lines: &[&str]) -> String {
    // Always end files with a single newline, and normalize to LF.
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_hook_with_snapshot_if_changed(path: &Path, existing: &str, updated: &str) -> Result<()> {
    if existing == updated {
        return Ok(());
    }

    create_hook_snapshot_and_prune(path, DEFAULT_MAX_SNAPSHOTS)?;
    fs::write(path, updated.as_bytes()).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn create_hook_snapshot_and_prune(hook_path: &Path, max_snapshots: usize) -> Result<()> {
    if !hook_path.is_file() {
        return Ok(());
    }

    let file_name = hook_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("Invalid hook path: {}", hook_path.display()))?;

    let parent = hook_path
        .parent()
        .ok_or_else(|| anyhow!("Invalid hook path (no parent): {}", hook_path.display()))?;

    let timestamp = format_timestamp_for_snapshot_name(OffsetDateTime::now_utc())?;
    let prefix = format!("{file_name}.snapshot-");
    let mut snapshot_path = parent.join(format!("{prefix}{timestamp}"));

    // Extremely unlikely, but ensure uniqueness.
    let mut counter: u32 = 0;
    while snapshot_path.exists() {
        counter = counter.saturating_add(1);
        if counter > 10_000 {
            return Err(anyhow!(
                "Too many snapshot files exist for {}",
                hook_path.display()
            ));
        }
        snapshot_path = parent.join(format!("{prefix}{timestamp}.{counter}"));
    }

    fs::copy(hook_path, &snapshot_path).with_context(|| {
        format!(
            "Failed to snapshot existing hook from {} to {}",
            hook_path.display(),
            snapshot_path.display()
        )
    })?;

    prune_hook_snapshots(parent, &prefix, max_snapshots)?;
    Ok(())
}

fn format_timestamp_for_snapshot_name(dt: OffsetDateTime) -> Result<String> {
    let fmt = format_description::parse("[year]-[month]-[day]-[hour]-[minute]-[second]")
        .context("Failed to build timestamp format")?;
    let timestamp = dt.format(&fmt).context("Failed to format timestamp")?;
    Ok(timestamp)
}

fn prune_hook_snapshots(hooks_dir: &Path, prefix: &str, max_snapshots: usize) -> Result<()> {
    if max_snapshots == 0 {
        return Ok(());
    }

    let entries = fs::read_dir(hooks_dir).with_context(|| {
        format!(
            "Failed to list hooks directory at {}",
            hooks_dir.display()
        )
    })?;

    let mut snapshots: Vec<String> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with(prefix) {
            continue;
        }
        snapshots.push(file_name.to_string());
    }

    // Lexicographic order matches chronological order for our timestamp format.
    snapshots.sort();

    if snapshots.len() <= max_snapshots {
        return Ok(());
    }

    let remove_count = snapshots.len() - max_snapshots;
    for file_name in snapshots.into_iter().take(remove_count) {
        let path = hooks_dir.join(&file_name);
        let _ = fs::remove_file(&path);
    }

    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn is_executable(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).ok()?;
    let mode = metadata.permissions().mode();
    Some((mode & 0o111) != 0)
}

#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn upsert_managed_block_inserts_after_shebang_when_missing() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\necho hi\n";
        let settings = ManagedPreCommitSettings {
            enabled: true,
            js_ts_tool: JsTsTool::Biome,
            python_tool: PythonTool::Ruff,
            java_kotlin_tool: JavaKotlinTool::Spotless,
            maybe_cargo_manifest_dir: None,
        };
        let repo_root = Path::new("/repo");
        let block = managed_pre_commit_block(&settings, repo_root);

        // act
        let updated = upsert_managed_block(existing, &block);

        // assert
        assert!(updated.starts_with("#!/bin/sh\n\n# >>> git-hook-installer managed block >>>"));
        Ok(())
    }

    #[test]
    fn uninstall_managed_block_removes_only_the_managed_section() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\n# >>> git-hook-installer managed block >>>\nGHI_ENABLED=1\n# <<< git-hook-installer managed block <<<\necho hi\n";

        // act
        let updated = uninstall_managed_block(existing)?;

        // assert
        assert!(updated.contains("echo hi"));
        assert!(!updated.contains(MANAGED_BLOCK_BEGIN));
        Ok(())
    }

    #[test]
    fn disable_managed_block_sets_enabled_to_zero() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\n# >>> git-hook-installer managed block >>>\nGHI_ENABLED=1\n# <<< git-hook-installer managed block <<<\n";

        // act
        let updated = disable_managed_block(existing)?;

        // assert
        assert!(updated.contains("GHI_ENABLED=0\n"));
        Ok(())
    }

    #[test]
    fn upsert_managed_pre_commit_hook_writes_executable_file() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let git_dir = temp.path().join(".git");
        fs::create_dir_all(git_dir.join("hooks"))?;

        let settings = ManagedPreCommitSettings {
            enabled: true,
            js_ts_tool: JsTsTool::Biome,
            python_tool: PythonTool::Ruff,
            java_kotlin_tool: JavaKotlinTool::Spotless,
            maybe_cargo_manifest_dir: None,
        };
        let repo_root = temp.path();
        let block = managed_pre_commit_block(&settings, repo_root);

        // act
        upsert_managed_pre_commit_hook(&git_dir, &block, InstallOptions { yes: true, non_interactive: true, force: true })?;

        // assert
        let hook_path = git_dir.join("hooks").join(PRE_COMMIT_HOOK_NAME);
        assert!(hook_path.is_file());
        Ok(())
    }

    #[test]
    fn writing_hook_creates_snapshot_and_prunes_oldest() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let hooks_dir = temp.path().join("hooks");
        fs::create_dir_all(&hooks_dir)?;
        let hook_path = hooks_dir.join("pre-commit");
        fs::write(&hook_path, "old\n")?;

        // Create 12 fake snapshots; pruning should keep 10.
        for i in 0..12 {
            // Use a fully sortable timestamp shape.
            let name = format!("pre-commit.snapshot-2026-01-11-15-{:02}-{:02}", i, 0);
            fs::write(hooks_dir.join(name), "snap\n")?;
        }

        let existing = fs::read_to_string(&hook_path)?;
        let updated = "new\n";

        // act
        write_hook_with_snapshot_if_changed(&hook_path, &existing, updated)?;

        // assert
        let mut snapshot_count = 0usize;
        for entry in fs::read_dir(&hooks_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with("pre-commit.snapshot-") {
                snapshot_count += 1;
            }
        }
        assert_eq!(snapshot_count, 10);
        Ok(())
    }
}

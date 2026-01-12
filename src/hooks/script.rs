use std::path::Path;

use crate::hooks::managed_block::{MANAGED_BLOCK_BEGIN, MANAGED_BLOCK_END};
use crate::hooks::types::{JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool};
use crate::util::relative_display;

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
        .map(|dir| relative_display(repo_root, dir))
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

pub fn shell_escape_path(path: &Path) -> String {
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


use std::path::{Path, PathBuf};

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::hooks::{JavaKotlinTool, JsTsTool, PythonTool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolChoiceKind {
    Detected,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolChoice<T> {
    pub tool: T,
    pub kind: ToolChoiceKind,
    pub maybe_reason: Option<&'static str>,
}

pub fn resolve_cargo_dir_best_effort(
    maybe_manifest_dir_from_cli: Option<&Path>,
    cwd: &Path,
    repo_root: &Path,
    options: ResolveHookOptions,
) -> Option<PathBuf> {
    let result = resolve_cargo_manifest_dir(maybe_manifest_dir_from_cli, cwd, repo_root, options);
    let Ok(cargo_dir) = result else {
        return None;
    };
    Some(cargo_dir)
}

pub fn choose_js_ts_tool(repo_root: &Path) -> ToolChoice<JsTsTool> {
    // Prefer Biome if a biome config exists; otherwise prefer Prettier/Eslint.
    let has_biome =
        repo_root.join("biome.json").is_file() || repo_root.join("biome.jsonc").is_file();
    if has_biome {
        return ToolChoice {
            tool: JsTsTool::Biome,
            kind: ToolChoiceKind::Detected,
            maybe_reason: Some("found biome.json/biome.jsonc"),
        };
    }

    // Prefer Prettier/Eslint if there are clear config signals.
    if has_prettier_or_eslint_config(repo_root) {
        return ToolChoice {
            tool: JsTsTool::PrettierEslint,
            kind: ToolChoiceKind::Detected,
            maybe_reason: Some("found Prettier/ESLint config"),
        };
    }

    ToolChoice {
        tool: JsTsTool::PrettierEslint,
        kind: ToolChoiceKind::Default,
        maybe_reason: None,
    }
}

pub fn detect_js_ts_repo_proof(repo_root: &Path) -> Option<&'static str> {
    // Strong signals at repo root.
    let root_signals = [
        "package.json",
        "tsconfig.json",
        "jsconfig.json",
        "deno.json",
        "deno.jsonc",
        "bun.lockb",
        "pnpm-lock.yaml",
        "yarn.lock",
        "package-lock.json",
    ];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found package/tooling file (package.json/tsconfig/jsconfig/lockfile)");
    }

    if repo_root.join("biome.json").is_file() || repo_root.join("biome.jsonc").is_file() {
        return Some("found biome.json/biome.jsonc");
    }

    if has_prettier_or_eslint_config(repo_root) {
        return Some("found Prettier/ESLint config");
    }

    // Common monorepo layout: tooling files may live in nested packages.
    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested package/tooling file (shallow scan)");
    }

    // Fallback: shallow scan for JS/TS source files. This is intentionally bounded to avoid
    // expensive repo walks.
    if has_any_file_with_ext_bounded(
        repo_root,
        &["js", "jsx", "ts", "tsx", "mjs", "cjs"],
        2,
        10_000,
    ) {
        return Some("found JS/TS source files (shallow scan)");
    }

    None
}

pub fn detect_python_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = [
        "pyproject.toml",
        "requirements.txt",
        "requirements-dev.txt",
        "setup.py",
        "setup.cfg",
        "Pipfile",
        "poetry.lock",
        "uv.lock",
    ];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found Python tooling file (pyproject/requirements/setup/Pipfile/lockfile)");
    }

    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested Python tooling file (shallow scan)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["py"], 2, 10_000) {
        return Some("found Python source files (shallow scan)");
    }

    None
}

pub fn detect_java_kotlin_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = [
        "gradlew",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "pom.xml",
    ];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found Gradle/Maven project file");
    }

    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested Gradle/Maven project file (shallow scan)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["java", "kt", "kts"], 2, 10_000) {
        return Some("found Java/Kotlin source files (shallow scan)");
    }

    None
}

pub fn detect_go_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = ["go.mod", "go.work", "go.sum"];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found Go module/work file (go.mod/go.work)");
    }

    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested Go module/work file (shallow scan)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["go"], 2, 10_000) {
        return Some("found Go source files (shallow scan)");
    }

    None
}

pub fn detect_ruby_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = ["Gemfile", "Gemfile.lock", ".ruby-version", "Rakefile"];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found Ruby tooling file (Gemfile/.ruby-version)");
    }

    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested Ruby tooling file (shallow scan)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["rb"], 2, 10_000) {
        return Some("found Ruby source files (shallow scan)");
    }

    None
}

pub fn detect_shell_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = [".shellcheckrc", ".shfmt"];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found shell tooling file (.shellcheckrc/.shfmt)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["sh", "bash", "zsh"], 2, 10_000) {
        return Some("found shell scripts (shallow scan)");
    }

    None
}

pub fn detect_terraform_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = [".terraform.lock.hcl"];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found Terraform lockfile (.terraform.lock.hcl)");
    }

    if has_any_file_named_bounded(repo_root, &root_signals, 3, 10_000) {
        return Some("found nested Terraform lockfile (shallow scan)");
    }

    if has_any_file_with_ext_bounded(repo_root, &["tf", "tfvars"], 2, 10_000) {
        return Some("found Terraform files (shallow scan)");
    }

    None
}

pub fn detect_c_cpp_repo_proof(repo_root: &Path) -> Option<&'static str> {
    let root_signals = [".clang-format"];
    if root_signals
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return Some("found clang-format config (.clang-format)");
    }

    if has_any_file_with_ext_bounded(
        repo_root,
        &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"],
        2,
        10_000,
    ) {
        return Some("found C/C++ files (shallow scan)");
    }

    None
}

pub fn detect_typescript_repo_proof(repo_root: &Path) -> Option<&'static str> {
    // Strong signal: a tsconfig exists (root or nested in common monorepo layouts).
    if repo_root.join("tsconfig.json").is_file() {
        return Some("found tsconfig.json");
    }
    if has_any_file_named_bounded(repo_root, &["tsconfig.json"], 3, 10_000) {
        return Some("found nested tsconfig.json (shallow scan)");
    }

    // Fallback: find TS sources in a bounded scan.
    if has_any_file_with_ext_bounded(repo_root, &["ts", "tsx"], 2, 10_000) {
        return Some("found TypeScript source files (shallow scan)");
    }

    None
}

pub fn choose_python_tool(repo_root: &Path) -> ToolChoice<PythonTool> {
    // Prefer Ruff if it appears configured; otherwise fall back to Black if configured.
    // Default to Ruff because it can both format and lint-fix.
    if has_ruff_config(repo_root) {
        return ToolChoice {
            tool: PythonTool::Ruff,
            kind: ToolChoiceKind::Detected,
            maybe_reason: Some("found ruff.toml/.ruff.toml or [tool.ruff] in pyproject.toml"),
        };
    }
    if has_black_config(repo_root) {
        return ToolChoice {
            tool: PythonTool::Black,
            kind: ToolChoiceKind::Detected,
            maybe_reason: Some("found black.toml or [tool.black] in pyproject.toml"),
        };
    }
    ToolChoice {
        tool: PythonTool::Ruff,
        kind: ToolChoiceKind::Default,
        maybe_reason: None,
    }
}

pub fn choose_java_kotlin_tool(repo_root: &Path) -> ToolChoice<JavaKotlinTool> {
    // Prefer Spotless if this looks like a Gradle project.
    if has_gradle_project(repo_root) {
        return ToolChoice {
            tool: JavaKotlinTool::Spotless,
            kind: ToolChoiceKind::Detected,
            maybe_reason: Some("found gradlew/build.gradle/build.gradle.kts"),
        };
    }

    ToolChoice {
        tool: JavaKotlinTool::Ktlint,
        kind: ToolChoiceKind::Default,
        maybe_reason: None,
    }
}

fn has_prettier_or_eslint_config(repo_root: &Path) -> bool {
    let prettier_configs = [
        ".prettierrc",
        ".prettierrc.json",
        ".prettierrc.yaml",
        ".prettierrc.yml",
        ".prettierrc.js",
        ".prettierrc.cjs",
        "prettier.config.js",
        "prettier.config.cjs",
        "prettier.config.mjs",
    ];
    if prettier_configs
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return true;
    }

    let eslint_configs = [
        ".eslintrc",
        ".eslintrc.json",
        ".eslintrc.yaml",
        ".eslintrc.yml",
        ".eslintrc.js",
        ".eslintrc.cjs",
        "eslint.config.js",
        "eslint.config.cjs",
        "eslint.config.mjs",
    ];
    if eslint_configs
        .iter()
        .any(|name| repo_root.join(name).is_file())
    {
        return true;
    }

    // Heuristic: some repos store config in package.json.
    let package_json = repo_root.join("package.json");
    let Ok(contents) = std::fs::read_to_string(&package_json) else {
        return false;
    };

    // Keep this heuristic intentionally simple: we're not parsing JSON, just looking for strong signals.
    contents.contains("\"eslintConfig\"")
        || contents.contains("\"prettier\"")
        || contents.contains("\"eslint\"")
}

fn has_ruff_config(repo_root: &Path) -> bool {
    if repo_root.join("ruff.toml").is_file() || repo_root.join(".ruff.toml").is_file() {
        return true;
    }

    let pyproject = repo_root.join("pyproject.toml");
    let Ok(contents) = std::fs::read_to_string(&pyproject) else {
        return false;
    };
    contents.contains("[tool.ruff]")
}

fn has_black_config(repo_root: &Path) -> bool {
    if repo_root.join("black.toml").is_file() {
        return true;
    }

    let pyproject = repo_root.join("pyproject.toml");
    let Ok(contents) = std::fs::read_to_string(&pyproject) else {
        return false;
    };
    contents.contains("[tool.black]")
}

fn has_gradle_project(repo_root: &Path) -> bool {
    repo_root.join("gradlew").is_file()
        || repo_root.join("build.gradle").is_file()
        || repo_root.join("build.gradle.kts").is_file()
}

fn has_any_file_named_bounded(
    repo_root: &Path,
    names: &[&str],
    max_dir_depth: usize,
    max_entries: usize,
) -> bool {
    let mut visited_entries = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(repo_root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            if visited_entries >= max_entries {
                return false;
            }
            visited_entries += 1;

            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };

            if file_type.is_file() {
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if names.iter().any(|candidate| name == *candidate) {
                    return true;
                }
                continue;
            }

            if !file_type.is_dir() {
                continue;
            }

            if depth >= max_dir_depth {
                continue;
            }

            let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            // Avoid scanning huge/unrelated directories.
            if matches!(
                dir_name,
                ".git" | "node_modules" | "target" | "dist" | "build" | ".venv" | "__pycache__"
            ) {
                continue;
            }

            stack.push((path, depth + 1));
        }
    }

    false
}

fn has_any_file_with_ext_bounded(
    repo_root: &Path,
    exts: &[&str],
    max_dir_depth: usize,
    max_entries: usize,
) -> bool {
    let mut visited_entries = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(repo_root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            if visited_entries >= max_entries {
                return false;
            }
            visited_entries += 1;

            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };

            if file_type.is_file() {
                let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                    continue;
                };
                if exts.iter().any(|candidate| ext.eq_ignore_ascii_case(candidate)) {
                    return true;
                }
                continue;
            }

            if !file_type.is_dir() {
                continue;
            }

            if depth >= max_dir_depth {
                continue;
            }

            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            // Avoid scanning huge/unrelated directories.
            if matches!(
                name,
                ".git" | "node_modules" | "target" | "dist" | "build" | ".venv" | "__pycache__"
            ) {
                continue;
            }

            stack.push((path, depth + 1));
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tempfile::TempDir;

    #[test]
    fn choose_js_ts_tool_detects_biome_when_biome_config_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("biome.json"), "{ }")?;

        // act
        let choice = choose_js_ts_tool(temp.path());

        // assert
        assert!(matches!(choice.tool, JsTsTool::Biome));
        assert_eq!(choice.kind, ToolChoiceKind::Detected);
        Ok(())
    }

    #[test]
    fn choose_js_ts_tool_detects_prettier_eslint_when_prettier_config_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join(".prettierrc"), "{}")?;

        // act
        let choice = choose_js_ts_tool(temp.path());

        // assert
        assert!(matches!(choice.tool, JsTsTool::PrettierEslint));
        assert_eq!(choice.kind, ToolChoiceKind::Detected);
        Ok(())
    }

    #[test]
    fn detect_js_ts_repo_proof_none_when_no_signals_exist() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;

        // act
        let maybe_reason = detect_js_ts_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_none());
        Ok(())
    }

    #[test]
    fn detect_js_ts_repo_proof_some_when_package_json_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("package.json"), "{ }")?;

        // act
        let maybe_reason = detect_js_ts_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn detect_js_ts_repo_proof_some_when_nested_package_json_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        let packages_dir = temp.path().join("packages").join("app");
        std::fs::create_dir_all(&packages_dir)?;
        std::fs::write(packages_dir.join("package.json"), "{ }")?;

        // act
        let maybe_reason = detect_js_ts_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn detect_python_repo_proof_some_when_pyproject_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("pyproject.toml"), "[build-system]\n")?;

        // act
        let maybe_reason = detect_python_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn detect_go_repo_proof_some_when_go_mod_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("go.mod"), "module example.com/x\n")?;

        // act
        let maybe_reason = detect_go_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn detect_ruby_repo_proof_some_when_gemfile_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'\n")?;

        // act
        let maybe_reason = detect_ruby_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn detect_typescript_repo_proof_some_when_tsconfig_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("tsconfig.json"), "{ }")?;

        // act
        let maybe_reason = detect_typescript_repo_proof(temp.path());

        // assert
        assert!(maybe_reason.is_some());
        Ok(())
    }

    #[test]
    fn choose_python_tool_detects_ruff_when_pyproject_has_tool_ruff() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.ruff]\nline-length = 88\n",
        )?;

        // act
        let choice = choose_python_tool(temp.path());

        // assert
        assert!(matches!(choice.tool, PythonTool::Ruff));
        assert_eq!(choice.kind, ToolChoiceKind::Detected);
        Ok(())
    }

    #[test]
    fn choose_python_tool_detects_black_when_pyproject_has_tool_black() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.black]\nline-length = 88\n",
        )?;

        // act
        let choice = choose_python_tool(temp.path());

        // assert
        assert!(matches!(choice.tool, PythonTool::Black));
        assert_eq!(choice.kind, ToolChoiceKind::Detected);
        Ok(())
    }
}

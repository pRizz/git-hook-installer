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

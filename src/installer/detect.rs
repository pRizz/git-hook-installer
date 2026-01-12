use std::path::{Path, PathBuf};

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::hooks::{JavaKotlinTool, JsTsTool, PythonTool};

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

pub fn default_js_ts_tool(repo_root: &Path) -> JsTsTool {
    // Prefer Biome if a biome config exists; otherwise prefer Prettier/Eslint.
    let has_biome =
        repo_root.join("biome.json").is_file() || repo_root.join("biome.jsonc").is_file();
    if has_biome {
        return JsTsTool::Biome;
    }

    // Prefer Prettier/Eslint if there are clear config signals.
    if has_prettier_or_eslint_config(repo_root) {
        return JsTsTool::PrettierEslint;
    }

    JsTsTool::PrettierEslint
}

pub fn default_python_tool(repo_root: &Path) -> PythonTool {
    // Prefer Ruff if it appears configured; otherwise fall back to Black if configured.
    // Default to Ruff because it can both format and lint-fix.
    if has_ruff_config(repo_root) {
        return PythonTool::Ruff;
    }
    if has_black_config(repo_root) {
        return PythonTool::Black;
    }
    PythonTool::Ruff
}

pub fn default_java_kotlin_tool(repo_root: &Path) -> JavaKotlinTool {
    // Prefer Spotless if this looks like a Gradle project.
    let has_gradle = repo_root.join("gradlew").is_file()
        || repo_root.join("build.gradle").is_file()
        || repo_root.join("build.gradle.kts").is_file();
    if has_gradle {
        return JavaKotlinTool::Spotless;
    }
    JavaKotlinTool::Ktlint
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
    if eslint_configs.iter().any(|name| repo_root.join(name).is_file()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tempfile::TempDir;

    #[test]
    fn default_js_ts_tool_prefers_biome_when_biome_config_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join("biome.json"), "{ }")?;

        // act
        let tool = default_js_ts_tool(temp.path());

        // assert
        assert!(matches!(tool, JsTsTool::Biome));
        Ok(())
    }

    #[test]
    fn default_js_ts_tool_prefers_prettier_eslint_when_prettier_config_exists() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(temp.path().join(".prettierrc"), "{}")?;

        // act
        let tool = default_js_ts_tool(temp.path());

        // assert
        assert!(matches!(tool, JsTsTool::PrettierEslint));
        Ok(())
    }

    #[test]
    fn default_python_tool_prefers_ruff_when_pyproject_has_tool_ruff() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.ruff]\nline-length = 88\n",
        )?;

        // act
        let tool = default_python_tool(temp.path());

        // assert
        assert!(matches!(tool, PythonTool::Ruff));
        Ok(())
    }

    #[test]
    fn default_python_tool_prefers_black_when_pyproject_has_tool_black() -> Result<()> {
        // arrange
        let temp = TempDir::new()?;
        std::fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.black]\nline-length = 88\n",
        )?;

        // act
        let tool = default_python_tool(temp.path());

        // assert
        assert!(matches!(tool, PythonTool::Black));
        Ok(())
    }
}


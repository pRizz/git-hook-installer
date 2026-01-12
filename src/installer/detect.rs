use std::path::{Path, PathBuf};

use crate::cargo_repo::{resolve_cargo_manifest_dir, ResolveHookOptions};
use crate::hooks::{JavaKotlinTool, JsTsTool};

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
    let has_biome = repo_root.join("biome.json").is_file() || repo_root.join("biome.jsonc").is_file();
    if has_biome {
        return JsTsTool::Biome;
    }
    JsTsTool::PrettierEslint
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


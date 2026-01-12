use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dialoguer::Select;

use crate::cargo_repo::ResolveHookOptions;
use crate::hooks::{JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool};

use super::detect::{default_java_kotlin_tool, default_js_ts_tool};

pub fn resolve_pre_commit_settings(
    repo_root: &Path,
    maybe_cargo_dir: Option<PathBuf>,
    options: ResolveHookOptions,
) -> Result<ManagedPreCommitSettings> {
    let default_js_ts = default_js_ts_tool(repo_root);
    let default_python = PythonTool::Ruff;
    let default_java_kotlin = default_java_kotlin_tool(repo_root);

    if options.non_interactive || options.yes {
        return Ok(ManagedPreCommitSettings {
            enabled: true,
            js_ts_tool: default_js_ts,
            python_tool: default_python,
            java_kotlin_tool: default_java_kotlin,
            maybe_cargo_manifest_dir: maybe_cargo_dir,
        });
    }

    let js_ts_tool = Select::new()
        .with_prompt("JS/TS: choose formatter/linter toolchain")
        .default(match default_js_ts {
            JsTsTool::Biome => 0,
            JsTsTool::PrettierEslint => 1,
        })
        .items(&["Biome (check --write)", "Prettier (write) + ESLint (--fix)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let js_ts_tool = match js_ts_tool {
        0 => JsTsTool::Biome,
        _ => JsTsTool::PrettierEslint,
    };

    let python_tool = Select::new()
        .with_prompt("Python: choose formatter/linter toolchain")
        .default(0)
        .items(&["ruff (format + check --fix)", "black (format only)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let python_tool = match python_tool {
        0 => PythonTool::Ruff,
        _ => PythonTool::Black,
    };

    let java_kotlin_tool = Select::new()
        .with_prompt("Java/Kotlin: choose formatter toolchain")
        .default(match default_java_kotlin {
            JavaKotlinTool::Spotless => 0,
            JavaKotlinTool::Ktlint => 1,
        })
        .items(&["Spotless (gradle spotlessApply)", "ktlint (-F on staged Kotlin files)"])
        .interact()
        .context("Failed to read selection from stdin")?;
    let java_kotlin_tool = match java_kotlin_tool {
        0 => JavaKotlinTool::Spotless,
        _ => JavaKotlinTool::Ktlint,
    };

    Ok(ManagedPreCommitSettings {
        enabled: true,
        js_ts_tool,
        python_tool,
        java_kotlin_tool,
        maybe_cargo_manifest_dir: maybe_cargo_dir,
    })
}


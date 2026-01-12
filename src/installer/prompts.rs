use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cargo_repo::ResolveHookOptions;
use crate::hooks::{JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool};

use super::detect::{
    choose_java_kotlin_tool, choose_js_ts_tool, choose_python_tool, ToolChoice, ToolChoiceKind,
};

fn print_tool_choice<T: Copy>(label: &str, choice: ToolChoice<T>, tool_display: &str) {
    match choice.kind {
        ToolChoiceKind::Detected => {
            if let Some(reason) = choice.maybe_reason {
                println!("Auto-selected {label}: {tool_display} ({reason})");
                return;
            }
            println!("Auto-selected {label}: {tool_display}");
        }
        ToolChoiceKind::Default => {
            println!("Defaulting {label}: {tool_display}");
        }
    }
}

pub fn resolve_pre_commit_settings(
    repo_root: &Path,
    maybe_cargo_dir: Option<PathBuf>,
    options: ResolveHookOptions,
) -> Result<ManagedPreCommitSettings> {
    let js_ts_choice = choose_js_ts_tool(repo_root);
    let python_choice = choose_python_tool(repo_root);
    let java_kotlin_choice = choose_java_kotlin_tool(repo_root);

    if !options.non_interactive {
        let js_ts_display = match js_ts_choice.tool {
            JsTsTool::Biome => "biome",
            JsTsTool::PrettierEslint => "prettier+eslint",
        };
        let python_display = match python_choice.tool {
            PythonTool::Ruff => "ruff",
            PythonTool::Black => "black",
        };
        let java_kotlin_display = match java_kotlin_choice.tool {
            JavaKotlinTool::Spotless => "spotless",
            JavaKotlinTool::Ktlint => "ktlint",
        };

        print_tool_choice("JS/TS toolchain", js_ts_choice, js_ts_display);
        print_tool_choice("Python toolchain", python_choice, python_display);
        print_tool_choice("Java/Kotlin toolchain", java_kotlin_choice, java_kotlin_display);
    }

    // We intentionally avoid prompting for toolchain selection:
    // - The managed hook auto-skips when there are no matching staged files.
    // - Each tool is skipped if it isn't available on PATH (or via npx where applicable).
    // - Repo settings are stored in the hook file; users can uninstall/reinstall to re-detect.
    //
    // If callers want "no questions asked", they can still use --yes / --non-interactive,
    // but the toolchain selection itself is always non-interactive.
    let _ = options;

    Ok(ManagedPreCommitSettings {
        enabled: true,
        js_ts_tool: js_ts_choice.tool,
        python_tool: python_choice.tool,
        java_kotlin_tool: java_kotlin_choice.tool,
        maybe_cargo_manifest_dir: maybe_cargo_dir,
    })
}


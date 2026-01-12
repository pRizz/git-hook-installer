use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cargo_repo::ResolveHookOptions;
use crate::hooks::{JavaKotlinTool, JsTsTool, ManagedPreCommitSettings, PythonTool};

use super::detect::{
    choose_java_kotlin_tool, choose_js_ts_tool, choose_python_tool, detect_c_cpp_repo_proof,
    detect_go_repo_proof, detect_java_kotlin_repo_proof, detect_python_repo_proof,
    detect_ruby_repo_proof, detect_shell_repo_proof, detect_terraform_repo_proof,
    detect_js_ts_repo_proof, ToolChoice, ToolChoiceKind,
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
    let maybe_js_ts_proof = detect_js_ts_repo_proof(repo_root);
    let maybe_python_proof = detect_python_repo_proof(repo_root);
    let maybe_java_kotlin_proof = detect_java_kotlin_repo_proof(repo_root);
    let maybe_go_proof = detect_go_repo_proof(repo_root);
    let maybe_shell_proof = detect_shell_repo_proof(repo_root);
    let maybe_terraform_proof = detect_terraform_repo_proof(repo_root);
    let maybe_c_cpp_proof = detect_c_cpp_repo_proof(repo_root);
    let maybe_ruby_proof = detect_ruby_repo_proof(repo_root);
    let js_ts_choice = choose_js_ts_tool(repo_root);
    let python_choice = choose_python_tool(repo_root);
    let java_kotlin_choice = choose_java_kotlin_tool(repo_root);

    if !options.non_interactive {
        if let Some(reason) = maybe_js_ts_proof {
            let js_ts_display = match js_ts_choice.tool {
                JsTsTool::Biome => "biome",
                JsTsTool::PrettierEslint => "prettier+eslint",
            };
            println!("Detected JS/TS repo signals ({reason})");
            print_tool_choice("JS/TS toolchain", js_ts_choice, js_ts_display);
        } else {
            println!("Skipping JS/TS toolchain (no JS/TS repo signals found)");
        }
        let python_display = match python_choice.tool {
            PythonTool::Ruff => "ruff",
            PythonTool::Black => "black",
        };
        let java_kotlin_display = match java_kotlin_choice.tool {
            JavaKotlinTool::Spotless => "spotless",
            JavaKotlinTool::Ktlint => "ktlint",
        };

        if let Some(reason) = maybe_python_proof {
            println!("Detected Python repo signals ({reason})");
            print_tool_choice("Python toolchain", python_choice, python_display);
        } else {
            println!("Skipping Python toolchain (no Python repo signals found)");
        }

        if let Some(reason) = maybe_java_kotlin_proof {
            println!("Detected Java/Kotlin repo signals ({reason})");
            print_tool_choice(
                "Java/Kotlin toolchain",
                java_kotlin_choice,
                java_kotlin_display,
            );
        } else {
            println!("Skipping Java/Kotlin toolchain (no Java/Kotlin repo signals found)");
        }

        if let Some(reason) = maybe_go_proof {
            println!("Enabling Go formatting (detected signals: {reason})");
        } else {
            println!("Disabling Go formatting (no Go repo signals found)");
        }

        if let Some(reason) = maybe_shell_proof {
            println!("Enabling shell formatting/linting (detected signals: {reason})");
        } else {
            println!("Disabling shell formatting/linting (no shell repo signals found)");
        }

        if let Some(reason) = maybe_terraform_proof {
            println!("Enabling Terraform formatting (detected signals: {reason})");
        } else {
            println!("Disabling Terraform formatting (no Terraform repo signals found)");
        }

        if let Some(reason) = maybe_c_cpp_proof {
            println!("Enabling C/C++ formatting (detected signals: {reason})");
        } else {
            println!("Disabling C/C++ formatting (no C/C++ repo signals found)");
        }

        if let Some(reason) = maybe_ruby_proof {
            println!("Enabling Ruby formatting (detected signals: {reason})");
        } else {
            println!("Disabling Ruby formatting (no Ruby repo signals found)");
        }
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
        maybe_js_ts_tool: maybe_js_ts_proof.map(|_| js_ts_choice.tool),
        maybe_python_tool: maybe_python_proof.map(|_| python_choice.tool),
        maybe_java_kotlin_tool: maybe_java_kotlin_proof.map(|_| java_kotlin_choice.tool),
        go_enabled: maybe_go_proof.is_some(),
        shell_enabled: maybe_shell_proof.is_some(),
        terraform_enabled: maybe_terraform_proof.is_some(),
        c_cpp_enabled: maybe_c_cpp_proof.is_some(),
        ruby_enabled: maybe_ruby_proof.is_some(),
        maybe_cargo_manifest_dir: maybe_cargo_dir,
    })
}

use std::path::PathBuf;

#[derive(Clone, Copy)]
pub struct InstallOptions {
    pub yes: bool,
    pub non_interactive: bool,
    pub force: bool,
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
    /// If `None`, the hook will not attempt JS/TS (or Prettier-based Markdown/YAML) formatting.
    pub maybe_js_ts_tool: Option<JsTsTool>,
    /// If `None`, the hook will not attempt Python formatting/linting.
    pub maybe_python_tool: Option<PythonTool>,
    /// If `None`, the hook will not attempt Java/Kotlin formatting/linting.
    pub maybe_java_kotlin_tool: Option<JavaKotlinTool>,
    pub go_enabled: bool,
    pub shell_enabled: bool,
    pub terraform_enabled: bool,
    pub c_cpp_enabled: bool,
    pub ruby_enabled: bool,
    /// If set, `cargo fmt` will run from this directory.
    pub maybe_cargo_manifest_dir: Option<PathBuf>,
}

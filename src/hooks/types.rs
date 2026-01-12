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
    pub js_ts_tool: JsTsTool,
    pub python_tool: PythonTool,
    pub java_kotlin_tool: JavaKotlinTool,
    /// If set, `cargo fmt` will run from this directory.
    pub maybe_cargo_manifest_dir: Option<PathBuf>,
}

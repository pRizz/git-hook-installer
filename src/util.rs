use std::path::Path;

pub fn normalize_newlines(input: &str) -> String {
    let mut normalized = input.replace("\r\n", "\n");
    if normalized.ends_with('\n') {
        return normalized;
    }

    normalized.push('\n');
    normalized
}

pub fn relative_display(base: &Path, path: &Path) -> String {
    let Ok(rel) = path.strip_prefix(base) else {
        return path.display().to_string();
    };

    if rel.as_os_str().is_empty() {
        return ".".to_string();
    }

    rel.display().to_string()
}

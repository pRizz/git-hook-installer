//! Utility functions for path and string manipulation.
//!
//! This module provides helper functions for common operations like
//! normalizing line endings and displaying relative paths.

use std::path::Path;

pub fn relative_display(base: &Path, path: &Path) -> String {
    let Ok(rel) = path.strip_prefix(base) else {
        return path.display().to_string();
    };

    if rel.as_os_str().is_empty() {
        return ".".to_string();
    }

    rel.display().to_string()
}

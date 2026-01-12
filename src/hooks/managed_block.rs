use anyhow::{anyhow, Result};

pub const MANAGED_BLOCK_BEGIN: &str = "# >>> git-hook-installer managed block >>>";
pub const MANAGED_BLOCK_END: &str = "# <<< git-hook-installer managed block <<<";

pub fn ensure_shebang(contents: &str) -> String {
    let first_line = contents.lines().next().unwrap_or_default();
    if first_line.starts_with("#!") {
        return contents.to_string();
    }
    format!("#!/bin/sh\n{contents}")
}

pub fn upsert_managed_block(existing: &str, block: &str) -> String {
    let mut lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let block_lines: Vec<&str> = block.lines().collect();

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start <= end => {
            lines.splice(start..=end, block_lines);
            ensure_shebang(&normalize_newline_join(&lines))
        }
        _ => {
            // No managed block: insert after shebang if present.
            let insert_at = if !lines.is_empty() && lines[0].starts_with("#!") {
                1
            } else {
                0
            };

            let mut out: Vec<&str> = Vec::with_capacity(lines.len() + block_lines.len() + 2);
            out.extend_from_slice(&lines[..insert_at]);
            if insert_at > 0 {
                out.push("");
            }
            out.extend_from_slice(&block_lines);
            out.push("");
            out.extend_from_slice(&lines[insert_at..]);
            ensure_shebang(&normalize_newline_join(&out))
        }
    }
}

pub fn uninstall_managed_block(existing: &str) -> Result<String> {
    let lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let (Some(start), Some(end)) = (start_idx, end_idx) else {
        return Err(anyhow!("No managed git-hook-installer block found in pre-commit hook"));
    };
    if start > end {
        return Err(anyhow!("Invalid managed block markers in pre-commit hook"));
    }

    let mut out = lines;
    out.splice(start..=end, []);
    Ok(normalize_newline_join(&out))
}

pub fn disable_managed_block(existing: &str) -> Result<String> {
    let lines: Vec<&str> = existing.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if *line == MANAGED_BLOCK_BEGIN {
            start_idx = Some(idx);
            continue;
        }
        if *line == MANAGED_BLOCK_END {
            end_idx = Some(idx);
            break;
        }
    }

    let (Some(start), Some(end)) = (start_idx, end_idx) else {
        return Err(anyhow!("No managed git-hook-installer block found in pre-commit hook"));
    };
    if start > end {
        return Err(anyhow!("Invalid managed block markers in pre-commit hook"));
    }

    let mut did_change = false;
    let mut out = Vec::with_capacity(lines.len());

    for (idx, line) in lines.iter().enumerate() {
        if idx < start || idx > end {
            out.push(*line);
            continue;
        }

        if line.trim_start().starts_with("GHI_ENABLED=") {
            out.push("GHI_ENABLED=0");
            did_change = true;
            continue;
        }

        out.push(*line);
    }

    if !did_change {
        return Err(anyhow!("Managed block found, but no GHI_ENABLED setting line was found"));
    }

    Ok(normalize_newline_join(&out))
}

fn normalize_newline_join(lines: &[&str]) -> String {
    // Always end files with a single newline, and normalize to LF.
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn upsert_managed_block_inserts_after_shebang_when_missing() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\necho hi\n";
        let block = format!(
            "{MANAGED_BLOCK_BEGIN}\nGHI_ENABLED=1\n{MANAGED_BLOCK_END}\n"
        );

        // act
        let updated = upsert_managed_block(existing, &block);

        // assert
        assert!(updated.starts_with("#!/bin/sh\n\n# >>> git-hook-installer managed block >>>"));
        Ok(())
    }

    #[test]
    fn uninstall_managed_block_removes_only_the_managed_section() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\n# >>> git-hook-installer managed block >>>\nGHI_ENABLED=1\n# <<< git-hook-installer managed block <<<\necho hi\n";

        // act
        let updated = uninstall_managed_block(existing)?;

        // assert
        assert!(updated.contains("echo hi"));
        assert!(!updated.contains(MANAGED_BLOCK_BEGIN));
        Ok(())
    }

    #[test]
    fn disable_managed_block_sets_enabled_to_zero() -> Result<()> {
        // arrange
        let existing = "#!/bin/sh\n# >>> git-hook-installer managed block >>>\nGHI_ENABLED=1\n# <<< git-hook-installer managed block <<<\n";

        // act
        let updated = disable_managed_block(existing)?;

        // assert
        assert!(updated.contains("GHI_ENABLED=0\n"));
        Ok(())
    }
}


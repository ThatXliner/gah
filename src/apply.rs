use std::io::Write;
use std::process::{Command, Stdio};

use thiserror::Error;

use crate::diff::{DiffFile, DiffLine, Hunk};

#[derive(Error, Debug)]
pub enum ApplyError {
    #[error("git apply failed: {0}")]
    GitApplyFailed(String),
    #[error("failed to execute git: {0}")]
    GitExecFailed(#[from] std::io::Error),
}

pub fn reconstruct_patch(file: &DiffFile, hunks: &[&Hunk]) -> String {
    let mut patch = String::new();

    // Diff header
    patch.push_str(&file.diff_header);
    patch.push('\n');

    // Each hunk
    for hunk in hunks {
        patch.push_str(&hunk.header);
        patch.push('\n');

        for line in &hunk.lines {
            match line {
                DiffLine::Context(s) => {
                    patch.push(' ');
                    patch.push_str(s);
                }
                DiffLine::Add(s) => {
                    patch.push('+');
                    patch.push_str(s);
                }
                DiffLine::Remove(s) => {
                    patch.push('-');
                    patch.push_str(s);
                }
            }
            patch.push('\n');
        }
    }

    patch
}

pub fn apply_hunks(file: &DiffFile, hunks: &[&Hunk], dry_run: bool) -> Result<(), ApplyError> {
    if hunks.is_empty() {
        return Ok(());
    }

    let patch = reconstruct_patch(file, hunks);

    let mut cmd = Command::new("git");
    cmd.args(["apply", "--cached"]);

    if dry_run {
        cmd.arg("--check");
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(patch.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApplyError::GitApplyFailed(stderr.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_reconstruct_patch() {
        let file = DiffFile {
            path: PathBuf::from("test.rs"),
            hunks: vec![],
            diff_header: "diff --git a/test.rs b/test.rs\n--- a/test.rs\n+++ b/test.rs".to_string(),
        };

        let hunk = Hunk {
            index: 1,
            header: "@@ -1,3 +1,4 @@".to_string(),
            old_start: 1,
            old_count: 3,
            new_start: 1,
            new_count: 4,
            lines: vec![
                DiffLine::Context("line1".to_string()),
                DiffLine::Remove("old".to_string()),
                DiffLine::Add("new".to_string()),
                DiffLine::Context("line3".to_string()),
            ],
            function_context: None,
        };

        let patch = reconstruct_patch(&file, &[&hunk]);

        assert!(patch.contains("diff --git a/test.rs b/test.rs"));
        assert!(patch.contains("@@ -1,3 +1,4 @@"));
        assert!(patch.contains(" line1"));
        assert!(patch.contains("-old"));
        assert!(patch.contains("+new"));
    }
}

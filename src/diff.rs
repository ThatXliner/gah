use std::path::PathBuf;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub index: usize,
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
    pub function_context: Option<String>,
}

impl Hunk {
    pub fn additions(&self) -> usize {
        self.lines.iter().filter(|l| matches!(l, DiffLine::Add(_))).count()
    }

    pub fn deletions(&self) -> usize {
        self.lines.iter().filter(|l| matches!(l, DiffLine::Remove(_))).count()
    }

    pub fn content(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l {
                DiffLine::Context(s) => format!(" {s}"),
                DiffLine::Add(s) => format!("+{s}"),
                DiffLine::Remove(s) => format!("-{s}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn new_end(&self) -> u32 {
        self.new_start + self.new_count.saturating_sub(1)
    }
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub path: PathBuf,
    pub hunks: Vec<Hunk>,
    pub diff_header: String,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("invalid hunk header: {0}")]
    #[allow(dead_code)]
    InvalidHunkHeader(String),
    #[error("no files in diff")]
    #[allow(dead_code)]
    NoFiles,
}

pub fn parse_diff(diff_output: &str) -> Result<Vec<DiffFile>, ParseError> {
    let mut files = Vec::new();
    let file_re = Regex::new(r"^diff --git a/(.+) b/(.+)$").unwrap();
    let hunk_re = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$").unwrap();

    let file_chunks: Vec<&str> = diff_output.split("\ndiff --git ").collect();

    for (i, chunk) in file_chunks.iter().enumerate() {
        let chunk = if i == 0 {
            chunk.strip_prefix("diff --git ").unwrap_or(chunk)
        } else {
            chunk
        };

        if chunk.trim().is_empty() {
            continue;
        }

        let full_chunk = format!("diff --git {chunk}");
        let lines: Vec<&str> = full_chunk.lines().collect();

        if lines.is_empty() {
            continue;
        }

        let Some(caps) = file_re.captures(lines[0]) else {
            continue;
        };

        let path = PathBuf::from(&caps[2]);

        // Check for binary file
        if lines.iter().any(|l| l.starts_with("Binary files")) {
            continue;
        }

        // Find diff header (everything before first hunk)
        let mut header_end = lines.len();
        for (j, line) in lines.iter().enumerate() {
            if line.starts_with("@@") {
                header_end = j;
                break;
            }
        }

        let diff_header = lines[..header_end].join("\n");

        // Parse hunks
        let mut hunks = Vec::new();
        let mut current_hunk: Option<Hunk> = None;
        let mut hunk_index = 0;

        for line in &lines[header_end..] {
            if let Some(caps) = hunk_re.captures(line) {
                if let Some(h) = current_hunk.take() {
                    hunks.push(h);
                }

                hunk_index += 1;
                let old_start: u32 = caps[1].parse().unwrap_or(0);
                let old_count: u32 = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap_or(1));
                let new_start: u32 = caps[3].parse().unwrap_or(0);
                let new_count: u32 = caps.get(4).map_or(1, |m| m.as_str().parse().unwrap_or(1));
                let func_ctx = caps.get(5).map(|m| m.as_str().trim().to_string());

                current_hunk = Some(Hunk {
                    index: hunk_index,
                    header: line.to_string(),
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                    function_context: if func_ctx.as_ref().is_some_and(|s| !s.is_empty()) {
                        func_ctx
                    } else {
                        None
                    },
                });
            } else if let Some(ref mut hunk) = current_hunk {
                let diff_line = if let Some(rest) = line.strip_prefix('+') {
                    DiffLine::Add(rest.to_string())
                } else if let Some(rest) = line.strip_prefix('-') {
                    DiffLine::Remove(rest.to_string())
                } else if let Some(rest) = line.strip_prefix(' ') {
                    DiffLine::Context(rest.to_string())
                } else if line.starts_with('\\') {
                    // "\ No newline at end of file" - skip
                    continue;
                } else {
                    DiffLine::Context(line.to_string())
                };
                hunk.lines.push(diff_line);
            }
        }

        if let Some(h) = current_hunk {
            hunks.push(h);
        }

        if !hunks.is_empty() {
            files.push(DiffFile {
                path,
                hunks,
                diff_header,
            });
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@ fn main() {
 fn main() {
-    println!("Hello");
+    println!("Hello, world!");
+    println!("Goodbye");
 }
"#;
        let files = parse_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(files[0].hunks.len(), 1);

        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.index, 1);
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 4);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 5);
        assert_eq!(hunk.additions(), 2);
        assert_eq!(hunk.deletions(), 1);
    }

    #[test]
    fn test_parse_multiple_hunks() {
        let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line1
-old2
+new2
 line3
@@ -10,3 +10,4 @@
 line10
 line11
+inserted
 line12
"#;
        let files = parse_diff(diff).unwrap();
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].index, 1);
        assert_eq!(files[0].hunks[1].index, 2);
    }

    #[test]
    fn test_skip_binary() {
        let diff = r#"diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
"#;
        let files = parse_diff(diff).unwrap();
        assert!(files.is_empty());
    }
}

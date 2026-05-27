use serde::Serialize;

use crate::diff::{DiffFile, DiffLine, Hunk};

#[derive(Serialize)]
pub struct JsonHunk {
    pub index: usize,
    pub anchor: String,
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub content: String,
    pub function_context: Option<String>,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Serialize)]
pub struct JsonOutput {
    pub file: String,
    pub hunks: Vec<JsonHunk>,
}

impl From<&Hunk> for JsonHunk {
    fn from(h: &Hunk) -> Self {
        JsonHunk {
            index: h.index,
            anchor: h.anchor.clone(),
            header: h.header.clone(),
            old_start: h.old_start,
            old_count: h.old_count,
            new_start: h.new_start,
            new_count: h.new_count,
            content: h.content(),
            function_context: h.function_context.clone(),
            additions: h.additions(),
            deletions: h.deletions(),
        }
    }
}

pub fn format_preview(file: &DiffFile) -> String {
    let mut out = String::new();

    for hunk in &file.hunks {
        out.push_str(&format!(
            "\x1b[1;36m[{}:\x1b[33m{}\x1b[1;36m]\x1b[0m {}\n",
            hunk.index, hunk.anchor, hunk.header
        ));

        for line in &hunk.lines {
            match line {
                DiffLine::Context(s) => out.push_str(&format!("   {s}\n")),
                DiffLine::Add(s) => out.push_str(&format!("\x1b[32m + {s}\x1b[0m\n")),
                DiffLine::Remove(s) => out.push_str(&format!("\x1b[31m - {s}\x1b[0m\n")),
            }
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "\x1b[1m{}\x1b[0m: {} hunk{}\n",
        file.path.display(),
        file.hunks.len(),
        if file.hunks.len() == 1 { "" } else { "s" }
    ));

    out
}

pub fn format_json(file: &DiffFile) -> String {
    let output = JsonOutput {
        file: file.path.display().to_string(),
        hunks: file.hunks.iter().map(JsonHunk::from).collect(),
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_json_all(files: &[DiffFile]) -> String {
    let outputs: Vec<JsonOutput> = files
        .iter()
        .map(|f| JsonOutput {
            file: f.path.display().to_string(),
            hunks: f.hunks.iter().map(JsonHunk::from).collect(),
        })
        .collect();

    serde_json::to_string_pretty(&outputs).unwrap_or_default()
}

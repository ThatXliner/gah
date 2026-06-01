use std::io::IsTerminal;

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

/// Detect whether gah is running under an AI coding agent.
///
/// Agents capture stdout as text and choke on ANSI escapes, so color must be
/// suppressed even when stdout happens to be a TTY. Mirrors the env probes used
/// by other agent-aware CLIs.
fn is_agent() -> bool {
    fn env_set(key: &str) -> bool {
        std::env::var_os(key).is_some_and(|v| !v.is_empty())
    }

    // Explicit, standardized signal.
    if env_set("AI_AGENT") {
        return true;
    }

    const AGENT_ENV_VARS: &[&str] = &[
        "CURSOR_TRACE_ID",
        "CURSOR_AGENT",
        "GEMINI_CLI",
        "CODEX_SANDBOX",
        "CODEX_CI",
        "CODEX_THREAD_ID",
        "ANTIGRAVITY_AGENT",
        "AUGMENT_AGENT",
        "OPENCODE_CLIENT",
        "CLAUDECODE",
        "CLAUDE_CODE",
        "REPL_ID",
        "COPILOT_MODEL",
        "COPILOT_ALLOW_ALL",
        "COPILOT_GITHUB_TOKEN",
    ];
    if AGENT_ENV_VARS.iter().any(|k| env_set(k)) {
        return true;
    }

    // Cursor's VS Code extension host running in agent-exec mode.
    if std::env::var("CURSOR_EXTENSION_HOST_ROLE").as_deref() == Ok("agent-exec") {
        return true;
    }

    // Devin marks its sandbox with a well-known path rather than an env var.
    if std::path::Path::new("/opt/.devin").exists() {
        return true;
    }

    false
}

pub fn format_preview(file: &DiffFile) -> String {
    let use_color = std::io::stdout().is_terminal() && !is_agent();
    format_preview_inner(file, use_color)
}

fn format_preview_inner(file: &DiffFile, use_color: bool) -> String {
    let mut out = String::new();

    for hunk in &file.hunks {
        if use_color {
            out.push_str(&format!(
                "\x1b[1;36m[{}:\x1b[33m{}\x1b[1;36m]\x1b[0m {}\n",
                hunk.index, hunk.anchor, hunk.header
            ));
        } else {
            out.push_str(&format!(
                "[{}:{}] {}\n",
                hunk.index, hunk.anchor, hunk.header
            ));
        }

        for line in &hunk.lines {
            match line {
                DiffLine::Context(s) => out.push_str(&format!("   {s}\n")),
                DiffLine::Add(s) => {
                    if use_color {
                        out.push_str(&format!("\x1b[32m + {s}\x1b[0m\n"));
                    } else {
                        out.push_str(&format!(" + {s}\n"));
                    }
                }
                DiffLine::Remove(s) => {
                    if use_color {
                        out.push_str(&format!("\x1b[31m - {s}\x1b[0m\n"));
                    } else {
                        out.push_str(&format!(" - {s}\n"));
                    }
                }
            }
        }
        out.push('\n');
    }

    if use_color {
        out.push_str(&format!(
            "\x1b[1m{}\x1b[0m: {} hunk{}\n",
            file.path.display(),
            file.hunks.len(),
            if file.hunks.len() == 1 { "" } else { "s" }
        ));
    } else {
        out.push_str(&format!(
            "{}: {} hunk{}\n",
            file.path.display(),
            file.hunks.len(),
            if file.hunks.len() == 1 { "" } else { "s" }
        ));
    }

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

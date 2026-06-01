mod apply;
mod diff;
mod filter;
mod output;
#[cfg(feature = "tree-sitter")]
mod symbols;

use std::path::PathBuf;
use std::process::{Command, exit};

use clap::{Parser, Subcommand};
use regex::Regex;

use crate::apply::apply_hunks;
use crate::diff::parse_diff;
use crate::filter::{HunkFilter, filter_hunks, parse_indices, parse_line_ranges};
use crate::output::{format_json, format_json_all, format_preview};

#[derive(Parser)]
#[command(name = "gah")]
#[command(about = "Non-interactive hunk-based staging for git")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Preview hunks in a file
    Preview {
        /// File to preview (or --all for all files)
        file: Option<PathBuf>,

        /// Preview all modified files
        #[arg(long)]
        all: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Split hunks as finely as possible (zero context) before previewing
        #[arg(long)]
        split: bool,
    },

    /// Stage specific hunks
    Add {
        /// File to stage hunks from
        file: PathBuf,

        /// Hunk indices to stage (e.g., 1,3,5 or 1-3,7)
        #[arg(long)]
        hunks: Option<String>,

        /// Content-hash anchors to stage (stable across re-runs)
        #[arg(long, short = 'a')]
        anchor: Option<Vec<String>>,

        /// Regex pattern to match hunk content
        #[arg(long)]
        grep: Option<String>,

        /// Invert grep match (exclude matching hunks)
        #[arg(long)]
        invert: bool,

        /// Line ranges in working tree (e.g., 100-150,200-250). Stages only the
        /// changed lines within the range, trimming each matched hunk.
        #[arg(long)]
        lines: Option<String>,

        /// Split hunks as finely as possible (zero context) before filtering,
        /// so anchors/indices/grep operate on the smallest possible chunks
        #[arg(long)]
        split: bool,

        /// AST symbol names to match (requires tree-sitter feature)
        #[arg(long, short = 's')]
        symbol: Option<Vec<String>>,

        /// Show what would be staged without actually staging
        #[arg(long)]
        dry_run: bool,

        /// Output as JSON (useful with --dry-run)
        #[arg(long)]
        json: bool,
    },
}

fn get_diff(file: Option<&PathBuf>, unified: Option<u32>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(["diff", "--no-color"]);

    if let Some(u) = unified {
        cmd.arg(format!("--unified={u}"));
    }

    if let Some(f) = file {
        cmd.arg("--").arg(f);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run git diff: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn main() {
    let cli = Cli::parse();

    // Check if in git repo
    let status = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output();

    if status.is_err() || !status.unwrap().status.success() {
        eprintln!("error: not a git repository");
        exit(1);
    }

    match cli.command {
        Commands::Preview {
            file,
            all,
            json,
            split,
        } => {
            if !all && file.is_none() {
                eprintln!("error: specify a file or use --all");
                exit(1);
            }

            let diff_output = match get_diff(file.as_ref(), if split { Some(0) } else { None }) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };

            if diff_output.trim().is_empty() {
                if let Some(f) = file {
                    eprintln!("No changes to stage for {}", f.display());
                } else {
                    eprintln!("No changes to stage");
                }
                exit(0);
            }

            let files = match parse_diff(&diff_output) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("error parsing diff: {e}");
                    exit(1);
                }
            };

            if files.is_empty() {
                eprintln!("No changes to stage");
                exit(0);
            }

            if json {
                if all {
                    println!("{}", format_json_all(&files));
                } else {
                    println!("{}", format_json(&files[0]));
                }
            } else {
                for f in &files {
                    print!("{}", format_preview(f));
                    if all && files.len() > 1 {
                        println!();
                    }
                }
            }
        }

        Commands::Add {
            file,
            hunks,
            anchor,
            grep,
            invert,
            lines,
            split,
            symbol,
            dry_run,
            json,
        } => {
            // Check for --symbol without feature
            #[cfg(not(feature = "tree-sitter"))]
            if symbol.is_some() {
                eprintln!("error: --symbol requires the 'tree-sitter' feature");
                eprintln!("hint: reinstall with: cargo install gah --features tree-sitter");
                exit(1);
            }

            // Require at least one filter
            let has_filter = hunks.is_some()
                || anchor.is_some()
                || grep.is_some()
                || lines.is_some()
                || symbol.is_some();
            if !has_filter {
                #[cfg(feature = "tree-sitter")]
                {
                    eprintln!("error: specify --hunks, --anchor, --grep, --lines, or --symbol");
                }
                #[cfg(not(feature = "tree-sitter"))]
                {
                    eprintln!("error: specify --hunks, --anchor, --grep, or --lines");
                }
                exit(1);
            }

            // --split needs fine-grained hunks: re-diff at zero context so each
            // change is its own hunk. --lines keeps the default context so the
            // trimmed patch retains surrounding anchor lines (a zero-context
            // single insertion is placed unreliably by `git apply`); trimming
            // to the exact range happens after selection (below).
            let unified = if split { Some(0) } else { None };

            let diff_output = match get_diff(Some(&file), unified) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };

            if diff_output.trim().is_empty() {
                eprintln!("No changes to stage for {}", file.display());
                exit(0);
            }

            let files = match parse_diff(&diff_output) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("error parsing diff: {e}");
                    exit(1);
                }
            };

            let diff_file = match files.into_iter().find(|f| f.path == file) {
                Some(f) => f,
                None => {
                    eprintln!("No changes to stage for {}", file.display());
                    exit(0);
                }
            };

            // Build filter
            let mut hunk_filter = HunkFilter::default();

            if let Some(ref h) = hunks {
                match parse_indices(h) {
                    Ok(indices) => {
                        // Validate indices
                        let max_index = diff_file.hunks.len();
                        for idx in &indices {
                            if *idx == 0 || *idx > max_index {
                                eprintln!(
                                    "error: hunk {idx} does not exist (file has {max_index} hunks)"
                                );
                                exit(1);
                            }
                        }
                        hunk_filter.indices = Some(indices);
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        exit(1);
                    }
                }
            }

            if let Some(ref anchors) = anchor {
                // Validate anchors exist
                for a in anchors {
                    let found = diff_file
                        .hunks
                        .iter()
                        .any(|h| h.anchor.starts_with(a) || a.starts_with(&h.anchor));
                    if !found {
                        eprintln!("error: no hunk matches anchor '{a}'");
                        exit(1);
                    }
                }
                hunk_filter.anchors = Some(anchors.clone());
            }

            if let Some(ref pattern) = grep {
                match Regex::new(pattern) {
                    Ok(re) => {
                        hunk_filter.grep = Some(re);
                        hunk_filter.grep_invert = invert;
                    }
                    Err(e) => {
                        eprintln!("error: invalid regex '{pattern}': {e}");
                        exit(1);
                    }
                }
            }

            if let Some(ref l) = lines {
                match parse_line_ranges(l) {
                    Ok(ranges) => hunk_filter.lines = Some(ranges),
                    Err(e) => {
                        eprintln!("error: {e}");
                        exit(1);
                    }
                }
            }

            #[cfg_attr(not(feature = "tree-sitter"), allow(unused_mut))]
            let mut selected = filter_hunks(&diff_file.hunks, &hunk_filter);

            // Apply symbol filter if specified (requires tree-sitter feature)
            #[cfg(feature = "tree-sitter")]
            if let Some(ref symbols_filter) = symbol {
                // Read the working tree file to extract symbols
                let source = match std::fs::read_to_string(&file) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error reading file for symbol extraction: {e}");
                        exit(1);
                    }
                };

                let symbols = match symbols::extract_symbols(&source, &file) {
                    Some(s) => s,
                    None => {
                        eprintln!(
                            "error: cannot extract symbols from {} (unsupported language or parse error)",
                            file.display()
                        );
                        exit(1);
                    }
                };

                // Filter hunks that touch any of the specified symbols
                selected = selected
                    .into_iter()
                    .filter(|h| {
                        symbols_filter.iter().any(|sym| {
                            symbols::hunk_matches_symbol(&symbols, h.new_start, h.new_end(), sym)
                        })
                    })
                    .collect();

                if selected.is_empty() {
                    eprintln!(
                        "error: no hunks match symbol(s): {}",
                        symbols_filter.join(", ")
                    );
                    exit(1);
                }
            }

            // Trim selected hunks to the exact line range. Each matched hunk is
            // restricted to its changed lines within the range; additions
            // outside become dropped, removals outside become context. Owned
            // results live in `trimmed` so `selected` can re-borrow them.
            let trimmed: Vec<crate::diff::Hunk>;
            if let Some(ref ranges) = hunk_filter.lines {
                trimmed = selected
                    .iter()
                    .filter_map(|h| {
                        ranges
                            .iter()
                            .find_map(|(start, end)| h.restrict_to_lines(*start, *end))
                    })
                    .collect();
                selected = trimmed.iter().collect();
            }

            if selected.is_empty() {
                if let Some(ref pattern) = grep {
                    eprintln!("No hunks match pattern '{pattern}'");
                } else {
                    eprintln!("No hunks match the specified filters");
                }
                exit(1);
            }

            if json {
                let json_hunks: Vec<_> = selected
                    .iter()
                    .map(|h| crate::output::JsonHunk::from(*h))
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "file": file.display().to_string(),
                        "hunks": json_hunks,
                        "dry_run": dry_run
                    }))
                    .unwrap()
                );
            }

            match apply_hunks(&diff_file, &selected, dry_run) {
                Ok(()) => {
                    if dry_run {
                        if !json {
                            println!(
                                "Would stage {} hunk{} from {}",
                                selected.len(),
                                if selected.len() == 1 { "" } else { "s" },
                                file.display()
                            );
                        }
                    } else {
                        println!(
                            "Staged {} hunk{} from {}",
                            selected.len(),
                            if selected.len() == 1 { "" } else { "s" },
                            file.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    eprintln!("hint: try --dry-run first to verify the patch");
                    exit(1);
                }
            }
        }
    }
}

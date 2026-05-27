mod apply;
mod diff;
mod filter;
mod output;

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

        /// Line ranges in working tree (e.g., 100-150,200-250)
        #[arg(long)]
        lines: Option<String>,

        /// Show what would be staged without actually staging
        #[arg(long)]
        dry_run: bool,

        /// Output as JSON (useful with --dry-run)
        #[arg(long)]
        json: bool,
    },
}

fn get_diff(file: Option<&PathBuf>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(["diff", "--no-color"]);

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
        Commands::Preview { file, all, json } => {
            if !all && file.is_none() {
                eprintln!("error: specify a file or use --all");
                exit(1);
            }

            let diff_output = match get_diff(file.as_ref()) {
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
            dry_run,
            json,
        } => {
            // Require at least one filter
            if hunks.is_none() && anchor.is_none() && grep.is_none() && lines.is_none() {
                eprintln!("error: specify --hunks, --anchor, --grep, or --lines");
                exit(1);
            }

            let diff_output = match get_diff(Some(&file)) {
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

            let selected = filter_hunks(&diff_file.hunks, &hunk_filter);

            if selected.is_empty() {
                if grep.is_some() {
                    eprintln!(
                        "No hunks match pattern '{}'",
                        grep.as_ref().unwrap()
                    );
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

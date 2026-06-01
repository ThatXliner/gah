use std::fs;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    dir
}

#[test]
fn test_preview_no_changes() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "hello\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["preview", "test.txt"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("No changes to stage"));
}

#[test]
fn test_preview_with_changes() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "line1\nline2\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "modified\nline2\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["preview", "test.txt"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[1:"))
        .stdout(predicate::str::contains("modified"));
}

#[test]
fn test_preview_json() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "modified\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["preview", "test.txt", "--json"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file\":"))
        .stdout(predicate::str::contains("\"hunks\":"));
}

#[test]
fn test_add_dry_run() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "modified\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["add", "test.txt", "--hunks", "1", "--dry-run"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Would stage"));

    // Verify nothing was actually staged
    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(status.stdout.is_empty());
}

#[test]
fn test_add_stages_hunk() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "modified\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["add", "test.txt", "--hunks", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Staged 1 hunk"));

    // Verify it was staged
    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("test.txt"));
}

#[test]
fn test_invalid_hunk_index() {
    let dir = setup_git_repo();

    fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "modified\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["add", "test.txt", "--hunks", "99"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_not_a_git_repo() {
    let dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["preview", "--all"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

fn commit_initial(dir: &TempDir, name: &str, content: &str) {
    fs::write(dir.path().join(name), content).unwrap();
    Command::new("git")
        .args(["add", name])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();
}

fn staged_content(dir: &TempDir, name: &str) -> String {
    let out = Command::new("git")
        .args(["show", &format!(":{name}")])
        .current_dir(dir.path())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn test_split_separates_distant_changes() {
    let dir = setup_git_repo();
    // Two changes far enough apart that --split yields two hunks.
    commit_initial(&dir, "f.txt", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
    fs::write(dir.path().join("f.txt"), "a\nB\nc\nd\ne\nf\ng\nh\nI\nj\n").unwrap();

    let mut cmd = Command::cargo_bin("gah").unwrap();
    cmd.args(["preview", "f.txt", "--split"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("2 hunks"));
}

#[test]
fn test_split_stages_one_change_only() {
    let dir = setup_git_repo();
    commit_initial(&dir, "f.txt", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
    fs::write(dir.path().join("f.txt"), "a\nB\nc\nd\ne\nf\ng\nh\nI\nj\n").unwrap();

    // Grab an anchor for the first change from --split preview json.
    let preview = Command::cargo_bin("gah")
        .unwrap()
        .args(["preview", "f.txt", "--split", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let first_anchor = json["hunks"][0]["anchor"].as_str().unwrap().to_string();

    Command::cargo_bin("gah")
        .unwrap()
        .args(["add", "f.txt", "--split", "-a", &first_anchor])
        .current_dir(dir.path())
        .assert()
        .success();

    // Only b->B staged; i->I untouched in the index.
    assert_eq!(
        staged_content(&dir, "f.txt"),
        "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\n"
    );
}

#[test]
fn test_lines_stages_single_changed_line_in_block() {
    let dir = setup_git_repo();
    // Adjacent replacement block: git can't split this even at -U0.
    commit_initial(&dir, "g.txt", "a\nb\nc\nd\ne\n");
    fs::write(dir.path().join("g.txt"), "a\nB2\nC3\nD4\ne\n").unwrap();

    Command::cargo_bin("gah")
        .unwrap()
        .args(["add", "g.txt", "--lines", "3"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Only new-line 3 (c->C3) staged; b and d remain original in the index.
    assert_eq!(staged_content(&dir, "g.txt"), "a\nb\nC3\nd\ne\n");
}

#[test]
fn test_lines_stages_single_insertion() {
    let dir = setup_git_repo();
    commit_initial(&dir, "h.txt", "a\nb\nc\n");
    // Insert X, Y, Z at new lines 2, 4, 6.
    fs::write(dir.path().join("h.txt"), "a\nX\nb\nY\nc\nZ\n").unwrap();

    Command::cargo_bin("gah")
        .unwrap()
        .args(["add", "h.txt", "--lines", "4"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Only Y inserted, at the correct position.
    assert_eq!(staged_content(&dir, "h.txt"), "a\nb\nY\nc\n");
}

#[test]
fn test_lines_with_unstaged_deletion_before_change() {
    let dir = setup_git_repo();
    // Delete b (new-file line accounting) AND change d->D2. The deletion is not
    // in the requested range, so it must be left out of the staged patch while
    // the d->D2 change at new-line 3 is still selected. The dropped deletion
    // must demote to context (not vanish) or `git apply` would fuzz/fail on the
    // now non-contiguous patch.
    commit_initial(&dir, "g.txt", "a\nb\nc\nd\ne\n");
    fs::write(dir.path().join("g.txt"), "a\nc\nD2\ne\n").unwrap();

    Command::cargo_bin("gah")
        .unwrap()
        .args(["add", "g.txt", "--lines", "3"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Only d->D2 staged; b stays in the index (its deletion is unstaged).
    assert_eq!(staged_content(&dir, "g.txt"), "a\nb\nc\nD2\ne\n");
}

#[test]
fn test_lines_no_match_in_range() {
    let dir = setup_git_repo();
    commit_initial(&dir, "g.txt", "a\nb\nc\n");
    fs::write(dir.path().join("g.txt"), "a\nB\nc\n").unwrap();

    Command::cargo_bin("gah")
        .unwrap()
        .args(["add", "g.txt", "--lines", "99"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No hunks match"));
}

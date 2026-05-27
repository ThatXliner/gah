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

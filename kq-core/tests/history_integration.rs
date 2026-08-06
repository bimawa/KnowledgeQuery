use std::fs;
use std::path::Path;
use std::process::Command;

use kq_core::history::{blame, diff, log};

// ---------------------------------------------------------------------------
// Helper: create a temporary git repository with sample commits
// ---------------------------------------------------------------------------

struct TempRepo {
    dir: tempfile::TempDir,
}

impl TempRepo {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let repo_path = dir.path();

        // git init
        Command::new("git").args(["init"]).current_dir(repo_path).output().expect("git init failed");

        // Configure user
        Command::new("git")
            .args(["config", "user.email", "integration@test.com"])
            .current_dir(repo_path)
            .output()
            .expect("git config user.email failed");
        Command::new("git")
            .args(["config", "user.name", "Integration Tester"])
            .current_dir(repo_path)
            .output()
            .expect("git config user.name failed");

        // Commit 1: Initial commit
        fs::write(repo_path.join("README.md"), "# Integration Repo\n").unwrap();
        fs::write(repo_path.join("notes.md"), "Some notes.\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git").args(["commit", "-m", "Initial commit"]).current_dir(repo_path).output().unwrap();

        // Commit 2: Update README and add a dir
        fs::write(repo_path.join("README.md"), "# Integration Repo\n\nUpdated.\n").unwrap();
        fs::create_dir_all(repo_path.join("docs")).unwrap();
        fs::write(repo_path.join("docs/guide.md"), "# Guide\n\nContent.\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add guide and update README"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Commit 3: Update guide only
        fs::write(repo_path.join("docs/guide.md"), "# Guide\n\nUpdated content.\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        Command::new("git").args(["commit", "-m", "Update guide"]).current_dir(repo_path).output().unwrap();

        TempRepo { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[test]
fn integration_log_returns_all_commits() {
    let repo = TempRepo::new();
    let entries = log(repo.path(), None, None).unwrap();
    assert_eq!(entries.len(), 3, "expected 3 commits");
    // Most recent first
    assert_eq!(entries[0].subject, "Update guide");
    assert_eq!(entries[1].subject, "Add guide and update README");
    assert_eq!(entries[2].subject, "Initial commit");
    // Check structure
    for entry in &entries {
        assert!(!entry.hash.is_empty(), "hash should not be empty");
        assert_eq!(entry.hash.len(), 7, "hash should be 7 chars");
        assert_eq!(entry.hash_long.len(), 40, "long hash should be 40 chars");
    }
}

#[test]
fn integration_log_with_limit() {
    let repo = TempRepo::new();
    let entries = log(repo.path(), Some(2), None).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].subject, "Update guide");
    assert_eq!(entries[1].subject, "Add guide and update README");
}

#[test]
fn integration_log_with_path_filter() {
    let repo = TempRepo::new();
    // README.md touched in commit 1 (created) and commit 2 (updated)
    let entries = log(repo.path(), None, Some("README.md")).unwrap();
    assert_eq!(entries.len(), 2, "expected 2 commits touching README.md");
    assert_eq!(entries[0].subject, "Add guide and update README");
    assert_eq!(entries[1].subject, "Initial commit");
}

#[test]
fn integration_log_with_nonexistent_path() {
    let repo = TempRepo::new();
    let entries = log(repo.path(), None, Some("no_such_file.txt")).unwrap();
    assert_eq!(entries.len(), 0);
}

#[test]
fn integration_diff_specific_commits() {
    let repo = TempRepo::new();
    let entries = log(repo.path(), None, None).unwrap();
    let from_hash = entries.iter().find(|e| e.subject == "Initial commit").unwrap().hash_long.clone();
    let to_hash = entries.iter().find(|e| e.subject == "Update guide").unwrap().hash_long.clone();

    // Diff README.md between initial commit and the final state.
    // The file went from 1 line to 3 lines (first line unchanged).
    let result = diff(repo.path(), "README.md", Some(&from_hash), Some(&to_hash)).unwrap();
    assert_eq!(result.path, "README.md");
    // First line stayed same, lines 2-3 added => 2 additions, 0 deletions
    assert!(result.additions >= 1, "expected at least 1 addition");
    assert!(!result.lines.is_empty(), "expected non-empty diff lines");

    // Verify DiffLine origins are valid
    for line in &result.lines {
        assert!(
            line.origin == '+' || line.origin == '-' || line.origin == ' ' || line.origin == 'H' || line.origin == 'F',
            "unexpected origin '{}'",
            line.origin
        );
    }
}

#[test]
fn integration_diff_default_args() {
    let repo = TempRepo::new();
    // Default diff (HEAD vs parent) for docs/guide.md
    let result = diff(repo.path(), "docs/guide.md", None, None).unwrap();
    assert_eq!(result.path, "docs/guide.md");
    // HEAD changed "Content." to "Updated content." — expect changes
    assert!(result.additions > 0 || result.deletions > 0, "expected non-empty diff");
}

#[test]
fn integration_diff_root_commit() {
    let repo = TempRepo::new();
    let entries = log(repo.path(), None, None).unwrap();
    let root_hash = entries.iter().find(|e| e.subject == "Initial commit").unwrap().hash_long.clone();

    // Diff root against empty tree
    let result = diff(repo.path(), "README.md", None, Some(&root_hash)).unwrap();
    assert!(result.additions > 0, "root commit should have additions against empty tree");
    assert_eq!(result.deletions, 0, "empty tree has nothing to delete");
}

#[test]
fn integration_blame_returns_annotations() {
    let repo = TempRepo::new();
    let entries = blame(repo.path(), "README.md").unwrap();
    assert!(!entries.is_empty(), "blame should return at least one entry");
    // README.md has 3 lines in the latest version
    assert_eq!(entries.len(), 3, "README.md should have 3 lines");

    for entry in &entries {
        assert!(entry.lineno >= 1, "line number should be >= 1");
        assert_eq!(entry.hash.len(), 7, "hash should be 7 chars");
        assert!(!entry.author.is_empty(), "author should not be empty");
        // Content can be empty for blank lines — that's correct
    }
}

#[test]
fn integration_blame_nonexistent_file() {
    let repo = TempRepo::new();
    let result = blame(repo.path(), "no_such_file.md");
    assert!(result.is_err(), "blame on nonexistent file should error");
}

#[test]
fn integration_empty_repo_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    Command::new("git").args(["init"]).current_dir(dir.path()).output().unwrap();

    let result = log(dir.path(), None, None);
    assert!(result.is_err(), "log on empty repo should error");
}

#[test]
fn integration_log_order_unchanged_across_calls() {
    let repo = TempRepo::new();
    let first = log(repo.path(), None, None).unwrap();
    let second = log(repo.path(), None, None).unwrap();
    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.hash_long, b.hash_long);
    }
}

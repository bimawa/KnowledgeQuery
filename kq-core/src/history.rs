use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use git2::{DiffFormat, DiffOptions, Repository, Sort};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single commit entry returned by [`log()`].
#[derive(Debug, Clone)]
pub struct CommitEntry {
    /// Short commit hash (first 7 hex characters).
    pub hash: String,
    /// Full commit hash (40 hex characters).
    pub hash_long: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_email: String,
    /// Commit timestamp.
    pub time: DateTime<Utc>,
    /// Commit subject (first line of message).
    pub subject: String,
}

/// A single line in a unified diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Origin: '+' for added, '-' for removed, ' ' for context, 'H' for hunk
    /// header, 'F' for file header, etc.
    pub origin: char,
    /// Old line number (None for added lines).
    pub old_lineno: Option<u32>,
    /// New line number (None for deleted lines).
    pub new_lineno: Option<u32>,
    /// Line content (without the leading origin character).
    pub content: String,
}

/// Aggregated diff result for a single file.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// File path relative to repo root.
    pub path: String,
    /// Number of added lines.
    pub additions: usize,
    /// Number of deleted lines.
    pub deletions: usize,
    /// Individual diff lines.
    pub lines: Vec<DiffLine>,
}

/// A single line annotated by [`blame()`].
#[derive(Debug, Clone)]
pub struct BlameEntry {
    /// Line number (1-indexed) in the current file.
    pub lineno: u32,
    /// Short commit hash (first 7 hex chars) of the last modification.
    pub hash: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_email: String,
    /// Commit time of the last modification.
    pub time: DateTime<Utc>,
    /// Content of this line.
    pub content: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a git2 `Time` into a chrono `DateTime<Utc>`.
///
/// libgit2 stores time as a Unix epoch offset in seconds *without* timezone
/// offset baked in (the timezone offset is stored separately).  We treat the
/// seconds value as UTC.
fn git_time_to_datetime(t: git2::Time) -> DateTime<Utc> {
    DateTime::from_timestamp(t.seconds(), 0).unwrap_or(Utc::now())
}

/// Shorten an Oid to its first 7 hex characters.
fn short_oid(oid: &git2::Oid) -> String {
    let s = oid.to_string();
    s[..7.min(s.len())].to_string()
}

/// Read the current content of a file at `repo_workdir / rel_path`.
///
/// Returns `Ok(Some(content))` on success, `Ok(None)` if the file doesn't exist
/// in the workdir, and `Err` on I/O errors other than "not found".
fn read_workdir_file(workdir: &Path, rel_path: &str) -> Result<Option<Vec<String>>> {
    let full = workdir.join(rel_path);
    match std::fs::read_to_string(&full) {
        Ok(text) => Ok(Some(text.lines().map(|l| l.to_string()).collect())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Failed to read workdir file '{}'", full.display())),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Return the commit log for the knowledge repository.
///
/// * `repo_path` — path to the git repository root.
/// * `limit` — maximum number of commits to return (`None` = all).
/// * `path` — optional file path filter: only commits touching this path
///   (relative to repo root) are included.
///
/// Commits are returned in reverse chronological order (most recent first).
pub fn log(repo_path: &Path, limit: Option<usize>, path: Option<&str>) -> Result<Vec<CommitEntry>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to open repository at '{}'", repo_path.display()))?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let mut entries: Vec<CommitEntry> = Vec::new();

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        // If a path filter is given, check whether this commit touches that
        // path.  We do this by diffing the commit against its parent(s).
        if let Some(filter_path) = path
            && !commit_touches_path(&repo, &commit, filter_path)
        {
            continue;
        }

        let author = commit.author();
        let entry = CommitEntry {
            hash: short_oid(&oid),
            hash_long: oid.to_string(),
            author: author.name().unwrap_or("unknown").to_string(),
            author_email: author.email().unwrap_or("unknown").to_string(),
            time: git_time_to_datetime(commit.time()),
            subject: commit.summary().unwrap_or("").to_string(),
        };
        entries.push(entry);

        // Respect the limit.
        if let Some(lim) = limit
            && entries.len() >= lim
        {
            break;
        }
    }

    Ok(entries)
}

/// Return a unified diff for a specific file between two commits.
///
/// * `repo_path` — path to the git repository root.
/// * `path` — file path relative to repo root (required).
/// * `from_commit` — starting commit revision (default: parent of HEAD).
/// * `to_commit` — ending commit revision (default: HEAD).
///
/// If `from_commit` is `None` and HEAD has no parent (root commit), the diff
/// is against the empty tree (all lines appear as additions).
pub fn diff(repo_path: &Path, path: &str, from_commit: Option<&str>, to_commit: Option<&str>) -> Result<DiffResult> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to open repository at '{}'", repo_path.display()))?;

    // Resolve the "to" (newer) side.
    let to_rev = to_commit.unwrap_or("HEAD");
    let to_obj = repo.revparse_single(to_rev)?;
    let to_commit_obj =
        to_obj.peel_to_commit().with_context(|| format!("'{}' does not resolve to a commit", to_rev))?;
    let to_tree = to_commit_obj.tree()?;

    // Resolve the "from" (older) side.
    let from_tree = match from_commit {
        Some(rev) => {
            let obj = repo.revparse_single(rev)?;
            let commit_obj = obj.peel_to_commit()?;
            Some(commit_obj.tree()?)
        }
        None => {
            // Default: parent of HEAD.
            if to_commit_obj.parent_count() > 0 {
                let parent = to_commit_obj.parent(0)?;
                Some(parent.tree()?)
            } else {
                // Root commit – diff against empty tree.
                None
            }
        }
    };

    let mut diff_opts = DiffOptions::new();
    diff_opts.pathspec(path);

    let diff = repo.diff_tree_to_tree(from_tree.as_ref(), Some(&to_tree), Some(&mut diff_opts))?;

    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut lines: Vec<DiffLine> = Vec::new();

    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        let old_lineno = line.old_lineno();
        let new_lineno = line.new_lineno();
        let content_bytes = line.content();
        // git2 includes trailing newlines; we strip them for cleaner output.
        let content_str = String::from_utf8_lossy(content_bytes).to_string();
        let content_trimmed =
            if content_str.ends_with('\n') { content_str[..content_str.len() - 1].to_string() } else { content_str };

        match origin {
            '+' => additions += 1,
            '-' => deletions += 1,
            _ => {}
        }

        lines.push(DiffLine { origin, old_lineno, new_lineno, content: content_trimmed });
        true
    })?;

    Ok(DiffResult { path: path.to_string(), additions, deletions, lines })
}

/// Return blame annotations (last modification per line) for a file.
///
/// * `repo_path` — path to the git repository root.
/// * `path` — file path relative to repo root (required).
///
/// Returns one [`BlameEntry`] per line in the current file content.
pub fn blame(repo_path: &Path, path: &str) -> Result<Vec<BlameEntry>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to open repository at '{}'", repo_path.display()))?;

    let workdir = repo.workdir().context("Repository is bare – blame requires a worktree")?;

    // Read the current file content so we can attach line content to each
    // blame entry.
    let file_lines = read_workdir_file(workdir, path)?;
    let file_lines = file_lines.unwrap_or_default();

    let blame = repo.blame_file(Path::new(path), None).with_context(|| format!("Failed to blame file '{}'", path))?;

    let mut entries: Vec<BlameEntry> = Vec::new();

    for hunk in blame.iter() {
        let oid = hunk.final_commit_id();
        let signature = hunk.final_signature();
        let start_line = hunk.final_start_line() as u32;
        let line_count = hunk.lines_in_hunk() as u32;

        let hash = short_oid(&oid);
        let author = signature.name().unwrap_or("unknown").to_string();
        let author_email = signature.email().unwrap_or("unknown").to_string();
        let time = git_time_to_datetime(signature.when());

        // Expand the hunk into individual lines.
        for offset in 0..line_count {
            let lineno = start_line + offset;
            let content = file_lines.get((lineno as usize).wrapping_sub(1)).cloned().unwrap_or_default();

            entries.push(BlameEntry {
                lineno,
                hash: hash.clone(),
                author: author.clone(),
                author_email: author_email.clone(),
                time,
                content,
            });
        }
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check whether `commit` touches `filter_path` (relative to repo root).
///
/// Returns `true` if any of the file's deltas in the commit (compared against
/// its first parent, or the empty tree for root commits) involve the given
/// path.
fn commit_touches_path(repo: &Repository, commit: &git2::Commit<'_>, filter_path: &str) -> bool {
    let tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return false,
    };

    let parent_tree = if commit.parent_count() > 0 { commit.parent(0).ok().and_then(|p| p.tree().ok()) } else { None };

    let mut diff_opts = DiffOptions::new();
    diff_opts.pathspec(filter_path);

    let diff = match repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_opts)) {
        Ok(d) => d,
        Err(_) => return false,
    };

    diff.deltas().len() > 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    /// Create a temporary git repository with sample commits for testing.
    struct TempRepo {
        dir: tempfile::TempDir,
    }

    impl TempRepo {
        fn new() -> Self {
            let dir = tempfile::TempDir::new().expect("failed to create temp dir");
            let repo_path = dir.path().to_path_buf();

            // git init
            Command::new("git").args(["init"]).current_dir(&repo_path).output().expect("git init failed");

            // Configure user
            Command::new("git")
                .args(["config", "user.email", "test@test.com"])
                .current_dir(&repo_path)
                .output()
                .expect("git config user.email failed");
            Command::new("git")
                .args(["config", "user.name", "Test User"])
                .current_dir(&repo_path)
                .output()
                .expect("git config user.name failed");

            // ---- Commit 1: Initial commit ----
            fs::write(repo_path.join("README.md"), "# Knowledge Repo\n").unwrap();
            Command::new("git").args(["add", "."]).current_dir(&repo_path).output().unwrap();
            Command::new("git").args(["commit", "-m", "Initial commit"]).current_dir(&repo_path).output().unwrap();

            // ---- Commit 2: Add guide and update README ----
            fs::write(repo_path.join("README.md"), "# Knowledge Repo\n\nSecond version.\n").unwrap();
            fs::create_dir_all(repo_path.join("docs")).unwrap();
            fs::write(repo_path.join("docs/guide.md"), "# Guide\n\nStart here.\n").unwrap();
            Command::new("git").args(["add", "."]).current_dir(&repo_path).output().unwrap();
            Command::new("git")
                .args(["commit", "-m", "Add guide and update README"])
                .current_dir(&repo_path)
                .output()
                .unwrap();

            // ---- Commit 3: Update guide ----
            fs::write(repo_path.join("docs/guide.md"), "# Guide\n\nUpdated content.\n").unwrap();
            Command::new("git").args(["add", "."]).current_dir(&repo_path).output().unwrap();
            Command::new("git").args(["commit", "-m", "Update guide"]).current_dir(&repo_path).output().unwrap();

            TempRepo { dir }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    #[test]
    fn test_log_returns_all_commits() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, None).unwrap();
        assert_eq!(entries.len(), 3, "expected 3 commits");
        // Most recent first
        assert_eq!(entries[0].subject, "Update guide");
        assert_eq!(entries[2].subject, "Initial commit");
    }

    #[test]
    fn test_log_with_limit() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), Some(1), None).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].subject, "Update guide");
    }

    #[test]
    fn test_log_order_is_deterministic() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, None).unwrap();
        let subjects: Vec<&str> = entries.iter().map(|e| e.subject.as_str()).collect();
        // Must contain all three in some order
        assert!(subjects.contains(&"Initial commit"));
        assert!(subjects.contains(&"Add guide and update README"));
        assert!(subjects.contains(&"Update guide"));
        // First entry should be the most recent
        assert_eq!(entries[0].subject, "Update guide");
    }

    #[test]
    fn test_log_with_path_filter() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, Some("README.md")).unwrap();
        // README.md was modified in commit 1 (initial) and commit 2 (update)
        assert_eq!(entries.len(), 2, "expected 2 commits touching README.md");
        for entry in &entries {
            assert!(!entry.hash.is_empty(), "hash should not be empty");
            assert!(!entry.author.is_empty(), "author should not be empty");
        }
    }

    #[test]
    fn test_log_with_nonexistent_path() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, Some("nonexistent.md")).unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_diff_between_commits() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, None).unwrap();
        // Find the "Initial commit" and "Update guide" by subject
        let from_entry = entries.iter().find(|e| e.subject == "Initial commit").unwrap();
        let to_entry = entries.iter().find(|e| e.subject == "Update guide").unwrap();

        let result =
            diff(repo.path(), "README.md", Some(from_entry.hash_long.as_str()), Some(to_entry.hash_long.as_str()))
                .unwrap();

        assert_eq!(result.path, "README.md");
        assert!(
            result.additions > 0 || result.deletions > 0,
            "expected non-empty diff between root and HEAD for README.md"
        );
        assert!(!result.lines.is_empty());
    }

    #[test]
    fn test_diff_default_to_parent() {
        let repo = TempRepo::new();
        // Default diff (HEAD vs parent) for README.md
        let result = diff(repo.path(), "README.md", None, None).unwrap();
        // HEAD (commit 3) does not touch README.md, so the diff should be empty
        assert!(result.lines.is_empty(), "README.md was not modified in HEAD");
    }

    #[test]
    fn test_diff_root_commit() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, None).unwrap();
        let root_entry = entries.iter().find(|e| e.subject == "Initial commit").unwrap();

        let result = diff(repo.path(), "README.md", None, Some(root_entry.hash_long.as_str())).unwrap();
        // Root commit against empty tree: all lines are additions
        assert!(result.additions > 0, "should have additions from empty tree");
    }

    #[test]
    fn test_blame_returns_lines() {
        let repo = TempRepo::new();
        let entries = blame(repo.path(), "README.md").unwrap();
        assert!(!entries.is_empty(), "blame should return at least one entry");
        for entry in &entries {
            assert!(entry.lineno >= 1, "line number should be >= 1");
            assert_eq!(entry.hash.len(), 7, "hash should be 7 chars");
            assert!(!entry.author.is_empty(), "author should not be empty");
        }
    }

    #[test]
    fn test_blame_nonexistent_file_returns_error() {
        let repo = TempRepo::new();
        let result = blame(repo.path(), "nonexistent.md");
        assert!(result.is_err(), "blame should error on nonexistent file");
    }

    #[test]
    fn test_empty_repo_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        Command::new("git").args(["init"]).current_dir(dir.path()).output().unwrap();

        let result = log(dir.path(), None, None);
        assert!(result.is_err(), "log on empty repo should error");
    }

    #[test]
    fn test_log_with_path_filter_guide() {
        let repo = TempRepo::new();
        let entries = log(repo.path(), None, Some("docs/guide.md")).unwrap();
        // docs/guide.md was created in commit 2 and modified in commit 3
        assert_eq!(entries.len(), 2, "expected 2 commits touching docs/guide.md");
        assert_eq!(entries[0].subject, "Update guide");
        assert_eq!(entries[1].subject, "Add guide and update README");
    }
}

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use git2::{Oid, Repository, Signature};

/// Open a git repository at the given path.
pub fn open_repo(path: &Path) -> Result<Repository> {
    Repository::open(path).with_context(|| format!("Failed to open git repository at {}", path.display()))
}

/// Stage all changes and create an auto-sync commit.
///
/// Returns `Ok(Some(oid))` if a commit was created,
/// `Ok(None)` if no changes were detected (tree matches HEAD).
pub fn auto_commit(repo: &Repository, dir: &Path) -> Result<Option<Oid>> {
    let mut index = repo.index().with_context(|| format!("Failed to open index for {}", dir.display()))?;

    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .with_context(|| format!("Failed to stage files in {}", dir.display()))?;
    index.write().with_context(|| format!("Failed to write index for {}", dir.display()))?;

    let tree_oid = index.write_tree().with_context(|| format!("Failed to write tree for {}", dir.display()))?;
    let tree = repo.find_tree(tree_oid).with_context(|| format!("Failed to find tree for {}", dir.display()))?;

    // Compare with HEAD to skip empty commits
    if let Ok(head) = repo.head()
        && let Ok(head_commit) = head.peel_to_commit()
        && let Ok(head_tree) = head_commit.tree()
        && head_tree.id() == tree_oid
    {
        return Ok(None); // No changes — skip commit
    }

    let signature = Signature::now("kq", "kq@knowledge").context("Failed to create git signature")?;

    let message = format!("docs: auto-sync [{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));

    let oid = match repo.head() {
        Ok(head_ref) => {
            let parent = head_ref.peel_to_commit().context("Failed to get HEAD commit")?;
            repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[&parent])
        }
        Err(_) => {
            // No commits yet — orphan (initial) commit
            repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[])
        }
    }
    .with_context(|| format!("Failed to create commit for {}", dir.display()))?;

    Ok(Some(oid))
}

/// Execute `auto_commit` with up to 3 retries and increasing delays.
///
/// Delays between attempts: 1s, 5s, 30s.
pub fn auto_commit_with_retry(repo: &Repository, dir: &Path) -> Result<Option<Oid>> {
    let delays = [Duration::from_secs(1), Duration::from_secs(5), Duration::from_secs(30)];

    let mut last_error = None;

    for attempt in 0..=delays.len() {
        match auto_commit(repo, dir) {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < delays.len() {
                    eprintln!(
                        "[kqs] Git commit failed (attempt {}/{}), retrying in {}s: {}",
                        attempt + 1,
                        delays.len() + 1,
                        delays[attempt].as_secs(),
                        last_error.as_ref().unwrap()
                    );
                    std::thread::sleep(delays[attempt]);
                }
            }
        }
    }

    Err(last_error.unwrap()).context("Git auto-commit failed after 3 retries")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_bare(dir: &Path) -> Repository {
        Repository::init(dir).unwrap()
    }

    fn init_with_commit(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        let sig = Signature::now("test", "test@test").unwrap();
        let mut index = repo.index().unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        // Return the repository by borrowing the tree first to avoid move conflict
        drop(tree);
        repo
    }

    #[test]
    fn test_open_repo_success() {
        let dir = TempDir::new().unwrap();
        // init_bare returns Repository (Result already unwrapped inside)
        let _repo = init_bare(dir.path());
        assert!(open_repo(dir.path()).is_ok());
    }

    #[test]
    fn test_open_repo_nonexistent() {
        let dir = TempDir::new().unwrap();
        assert!(open_repo(&dir.path().join("no_such_dir")).is_err());
    }

    #[test]
    fn test_auto_commit_no_parent() {
        let dir = TempDir::new().unwrap();
        let repo = init_bare(dir.path());
        fs::write(dir.path().join("hello.md"), "# Hello\n").unwrap();

        let result = auto_commit(&repo, dir.path()).unwrap();
        assert!(result.is_some(), "first commit should succeed");
    }

    #[test]
    fn test_auto_commit_no_changes() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        let result = auto_commit(&repo, dir.path()).unwrap();
        assert!(result.is_none(), "should skip when no changes");
    }

    #[test]
    fn test_auto_commit_with_changes() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        fs::write(dir.path().join("note.md"), "# Note\n").unwrap();

        let oid = auto_commit(&repo, dir.path()).unwrap();
        assert!(oid.is_some(), "should create commit");

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let msg = head.message().unwrap();
        assert!(msg.starts_with("docs: auto-sync ["), "msg: {msg}");
    }

    #[test]
    fn test_auto_commit_then_noop() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        fs::write(dir.path().join("a.md"), "content").unwrap();
        assert!(auto_commit(&repo, dir.path()).unwrap().is_some());
        assert!(auto_commit(&repo, dir.path()).unwrap().is_none());
    }

    #[test]
    fn test_auto_commit_update() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        let f = dir.path().join("d.md");
        fs::write(&f, "v1").unwrap();
        assert!(auto_commit(&repo, dir.path()).unwrap().is_some());
        fs::write(&f, "v2").unwrap();
        assert!(auto_commit(&repo, dir.path()).unwrap().is_some());
    }
}

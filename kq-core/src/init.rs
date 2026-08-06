use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use kq_config::KnowledgeConfig;

/// Initialize a new knowledge repository at the given path.
///
/// This is the core implementation backing `kqs init`. It:
/// - Resolves the target directory (default: `~/.knowledge/`)
/// - Creates the directory structure (`docs/`, `tasks/`, `.kq/`)
/// - Initializes a git repository
/// - Writes a default `knowledge.toml` configuration
/// - Stages all files and creates an initial commit
/// - Optionally adds a remote
///
/// # Arguments
///
/// * `path` - Optional explicit path. If `None`, `~/.knowledge/` is used.
/// * `remote` - Optional git remote URL to add as `origin`.
/// * `force` - If `true`, reinitialize even if a git repo already exists
///   (removes the existing `.git` directory).
///
/// # Errors
///
/// Returns an error if:
/// - The directory already contains a `.git` directory and `force` is `false`.
/// - Filesystem operations fail (permission denied, etc.).
/// - Git operations fail.
pub fn init(path: Option<PathBuf>, remote: Option<String>, force: bool) -> Result<()> {
    // 1. Determine the target path
    let target_path = resolve_path(path)?;

    // 2. Check for existing git repository
    let git_dir = target_path.join(".git");
    let repo_exists = git_dir.exists();

    if repo_exists {
        if !force {
            anyhow::bail!(
                "Knowledge repository already exists at {}. Use --force to reinitialize.",
                target_path.display()
            );
        }
        // Remove the .git directory
        fs::remove_dir_all(&git_dir)
            .with_context(|| format!("Failed to remove existing .git directory at {}", git_dir.display()))?;
    }

    // 3. Create directory structure
    fs::create_dir_all(target_path.join("docs")).context("Failed to create docs/ directory")?;
    fs::create_dir_all(target_path.join("tasks")).context("Failed to create tasks/ directory")?;
    fs::create_dir_all(target_path.join(".kq")).context("Failed to create .kq/ directory")?;
    // 3.1 Create default templates
    crate::docs::init_templates(&target_path)?;
    // 3.2 Create events directory
    fs::create_dir_all(target_path.join(".kq/events")).context("Failed to create .kq/events/ directory")?;

    // 3.5 Create SQLite database with FTS5 schema
    let db_path = target_path.join(".kq/knowledge.db");
    crate::db::init_db(&db_path).context("Failed to initialize FTS database")?;

    // 4. Initialize git repository
    let repo = git2::Repository::init(&target_path)
        .with_context(|| format!("Failed to initialize git repository at {}", target_path.display()))?;

    // 5. Generate default knowledge.toml
    let config = KnowledgeConfig { knowledge_path: PathBuf::from("."), ..KnowledgeConfig::default() };
    let toml_str = config.to_toml_string().context("Failed to serialize default configuration to TOML")?;

    // 6. Write knowledge.toml
    let config_path = target_path.join("knowledge.toml");
    fs::write(&config_path, &toml_str)
        .with_context(|| format!("Failed to write config file at {}", config_path.display()))?;

    // 7. Stage all files
    let mut index = repo.index().context("Failed to open git index")?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).context("Failed to stage files with git add")?;
    index.write().context("Failed to write git index")?;

    // 8. Create initial commit
    let tree_oid = index.write_tree().context("Failed to write git tree")?;
    let tree = repo.find_tree(tree_oid).context("Failed to find git tree")?;
    let signature = git2::Signature::now("kq init", "kq@local").context("Failed to create git signature")?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "init",
        &tree,
        &[], // no parent commits for initial commit
    )
    .context("Failed to create initial commit")?;

    // 9. Optionally add remote
    if let Some(url) = remote {
        repo.remote("origin", &url).with_context(|| format!("Failed to add remote 'origin' with URL: {}", url))?;
    }

    // Index existing files
    let _ = crate::indexer::index_all(&target_path);
    Ok(())
}

///
/// If `path` is `Some`, it is returned as-is.
/// If `path` is `None`, the current working directory is used.
fn resolve_path(path: Option<PathBuf>) -> Result<PathBuf> {
    Ok(path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Helper to create a unique temporary directory for each test.
    fn unique_temp_dir() -> PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("kq_init_test_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_resolve_path_with_explicit_path() {
        let path = PathBuf::from("/tmp/my_knowledge");
        let resolved = resolve_path(Some(path.clone())).unwrap();
        assert_eq!(resolved, path);
    }

    #[test]
    fn test_resolve_path_with_none_uses_cwd() {
        let resolved = resolve_path(None).unwrap();
        assert_eq!(resolved, std::env::current_dir().unwrap());
    }

    #[test]
    fn test_init_creates_git_repo() {
        let test_path = unique_temp_dir();
        init(Some(test_path.clone()), None, false).unwrap();

        assert!(test_path.join(".git").is_dir());

        // Verify it's a valid git repo
        let repo = git2::Repository::open(&test_path).unwrap();
        let head = repo.head().unwrap();
        assert!(head.peel_to_commit().is_ok());

        fs::remove_dir_all(&test_path).ok();
    }

    #[test]
    fn test_init_errors_on_existing_repo_without_force() {
        let test_path = unique_temp_dir();
        init(Some(test_path.clone()), None, false).unwrap();

        // Second init without force should fail
        let result = init(Some(test_path.clone()), None, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already exists"));
        assert!(err.contains("--force"));

        fs::remove_dir_all(&test_path).ok();
    }

    #[test]
    fn test_init_force_reinitializes() {
        let test_path = unique_temp_dir();
        init(Some(test_path.clone()), None, false).unwrap();

        // Reinitialize with force should succeed
        init(Some(test_path.clone()), None, true).unwrap();

        // Repo should still be valid after re-init
        assert!(test_path.join(".git").is_dir());
        let repo = git2::Repository::open(&test_path).unwrap();
        assert!(repo.head().unwrap().peel_to_commit().is_ok());

        fs::remove_dir_all(&test_path).ok();
    }

    #[test]
    fn test_init_adds_remote() {
        let test_path = unique_temp_dir();
        let remote_url = "https://example.com/user/knowledge.git";
        init(Some(test_path.clone()), Some(remote_url.to_string()), false).unwrap();

        let repo = git2::Repository::open(&test_path).unwrap();
        let remote = repo.find_remote("origin").unwrap();
        assert_eq!(remote.url().unwrap(), remote_url);

        fs::remove_dir_all(&test_path).ok();
    }

    #[test]
    fn test_init_handles_nonexistent_parent_dir() {
        let test_path = unique_temp_dir().join("deeply").join("nested").join("repo");
        init(Some(test_path.clone()), None, false).unwrap();

        assert!(test_path.join("docs").is_dir());
        assert!(test_path.join(".git").is_dir());

        fs::remove_dir_all(&test_path).ok();
    }
    #[test]
    fn test_knowledge_toml_is_valid() {
        let test_path = unique_temp_dir();
        init(Some(test_path.clone()), None, false).unwrap();

        let config = KnowledgeConfig::load(test_path.join("knowledge.toml")).unwrap();
        assert_eq!(config.knowledge_path, PathBuf::from("."));

        fs::remove_dir_all(&test_path).ok();
    }
}

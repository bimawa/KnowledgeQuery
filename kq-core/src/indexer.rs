use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

/// Report of indexing operations.
#[derive(Debug, Clone, Default)]
pub struct IndexingReport {
    /// Number of files successfully indexed.
    pub indexed: usize,
    /// Number of files skipped (no change).
    pub skipped: usize,
    /// Number of files that failed to index.
    pub failed: usize,
    /// Paths of files that failed, with error messages.
    pub errors: Vec<(String, String)>,
}

/// Walk `dir` recursively, find all `.md` files, and index them.
///
/// Uses `walkdir::WalkDir` for directory traversal. For each `.md` file,
/// calls `index_file()` and collects results into an `IndexingReport`.
///
/// # Errors
/// - Database connection cannot be obtained.
pub fn index_all(dir: &Path) -> Result<IndexingReport> {
    let conn = crate::db::get_db()?;
    index_all_internal(&conn, dir)
}

/// Internal version of [`index_all`] that takes an explicit connection.
pub(crate) fn index_all_internal(conn: &Connection, dir: &Path) -> Result<IndexingReport> {
    let mut report = IndexingReport::default();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|ext| ext == "md").unwrap_or(false) && path.is_file() {
            match index_file_internal(conn, path) {
                Ok(true) => report.indexed += 1,
                Ok(false) => report.skipped += 1,
                Err(e) => {
                    report.failed += 1;
                    report.errors.push((path.to_string_lossy().to_string(), e.to_string()));
                }
            }
        }
    }

    // Re-index vector embeddings for changed files
    let _ = crate::vector::reindex_embeddings(conn);
    Ok(report)
}

/// Index a single file: read content, compute SHA256 hash, check if
/// re-indexing is needed, and update the database.
///
/// Returns `Ok(true)` if the file was indexed, `Ok(false)` if it was skipped
/// (hash unchanged).
///
/// # Arguments
/// * `path` — Filesystem path to the `.md` file.
///
/// # Errors
/// - File cannot be read.
/// - Hash computation fails.
/// - Database query/insert fails.
pub fn index_file(path: &Path) -> Result<bool> {
    let conn = crate::db::get_db()?;
    index_file_internal(&conn, path)
}

/// Internal version of [`index_file`] that takes an explicit connection.
pub(crate) fn index_file_internal(conn: &Connection, path: &Path) -> Result<bool> {
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
    let hash = sha256_hex(&content);
    let path_str = path.to_string_lossy();

    if !needs_reindex(conn, &path_str, &hash)? {
        return Ok(false);
    }

    // Use ISO 8601 format for last_modified
    let last_modified = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET content_hash = ?2, last_modified = ?3, content = ?4",
        rusqlite::params![path_str.as_ref(), hash, last_modified, content],
    )?;

    Ok(true)
}

/// Check whether a file needs re-indexing by comparing its content hash
/// against the stored hash in the database.
///
/// Returns `true` if:
/// - The file path is not in the database.
/// - The stored hash differs from the given hash.
///
/// Returns `false` if the hash matches the stored hash.
///
/// # Errors
/// - Database query fails.
pub fn needs_reindex(conn: &Connection, path: &str, hash: &str) -> Result<bool> {
    let stored: Option<String> = conn
        .query_row("SELECT content_hash FROM files WHERE path = ?1", rusqlite::params![path], |row| row.get(0))
        .ok();

    match stored {
        Some(h) => Ok(h != hash),
        None => Ok(true),
    }
}

/// Compute the SHA256 hex digest of a string.
fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;
    use std::fs;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, Connection) {
        let dir = TempDir::new().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        create_schema(&conn).unwrap();
        (dir, conn)
    }

    #[test]
    fn test_needs_reindex_new_file() {
        let (_dir, conn) = setup_db();
        assert!(needs_reindex(&conn, "/new/path.md", "abc123").unwrap());
    }

    #[test]
    fn test_needs_reindex_matching_hash() {
        let (_dir, conn) = setup_db();
        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["/test.md", "abc123", "2026-01-01", "content"],
        )
        .unwrap();
        assert!(!needs_reindex(&conn, "/test.md", "abc123").unwrap());
    }

    #[test]
    fn test_needs_reindex_different_hash() {
        let (_dir, conn) = setup_db();
        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["/test.md", "abc123", "2026-01-01", "content"],
        )
        .unwrap();
        assert!(needs_reindex(&conn, "/test.md", "def456").unwrap());
    }

    #[test]
    fn test_sha256_hex_non_empty() {
        let hash = sha256_hex("hello world");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_sha256_hex_known_value() {
        // SHA256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let hash = sha256_hex("hello");
        assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_index_file_internal_creates_entry() {
        let (_dir, conn) = setup_db();
        let file_dir = TempDir::new().unwrap();
        let file_path = file_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World\n").unwrap();

        let result = index_file_internal(&conn, &file_path).unwrap();
        assert!(result, "file should have been indexed");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                rusqlite::params![file_path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_index_file_internal_skips_unchanged() {
        let (_dir, conn) = setup_db();
        let file_dir = TempDir::new().unwrap();
        let file_path = file_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World\n").unwrap();

        // First index
        let result1 = index_file_internal(&conn, &file_path).unwrap();
        assert!(result1);

        // Second index (same content)
        let result2 = index_file_internal(&conn, &file_path).unwrap();
        assert!(!result2, "unchanged file should be skipped");
    }

    #[test]
    fn test_index_file_internal_updates_on_change() {
        let (_dir, conn) = setup_db();
        let file_dir = TempDir::new().unwrap();
        let file_path = file_dir.path().join("test.md");
        fs::write(&file_path, "# Version 1\n").unwrap();

        // First index
        let result1 = index_file_internal(&conn, &file_path).unwrap();
        assert!(result1);

        // Modify file
        fs::write(&file_path, "# Version 2\n").unwrap();

        // Second index (different content)
        let result2 = index_file_internal(&conn, &file_path).unwrap();
        assert!(result2, "changed file should be re-indexed");
    }

    #[test]
    fn test_index_all_internal_finds_md_files() {
        let (_dir, conn) = setup_db();
        let file_dir = TempDir::new().unwrap();
        fs::write(file_dir.path().join("a.md"), "a").unwrap();
        fs::write(file_dir.path().join("b.md"), "b").unwrap();
        fs::write(file_dir.path().join("c.txt"), "c").unwrap(); // should be ignored
        fs::create_dir_all(file_dir.path().join("sub")).unwrap();
        fs::write(file_dir.path().join("sub/d.md"), "d").unwrap();

        let report = index_all_internal(&conn, file_dir.path()).unwrap();
        assert_eq!(report.indexed, 3, "should index 3 .md files");
        assert_eq!(report.skipped, 0);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn test_index_all_internal_skips_unchanged() {
        let (_dir, conn) = setup_db();
        let file_dir = TempDir::new().unwrap();
        fs::write(file_dir.path().join("a.md"), "same content").unwrap();
        fs::write(file_dir.path().join("b.md"), "more content").unwrap();

        // First pass
        let report1 = index_all_internal(&conn, file_dir.path()).unwrap();
        assert_eq!(report1.indexed, 2);

        // Second pass (no changes)
        let report2 = index_all_internal(&conn, file_dir.path()).unwrap();
        assert_eq!(report2.indexed, 0);
        assert_eq!(report2.skipped, 2);
    }
}

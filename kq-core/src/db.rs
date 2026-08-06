use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use rusqlite::Connection;

/// Global database connection, initialized once via [`init_db()`].
static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// Initialize the SQLite database at `db_path`.
///
/// Creates the database file, establishes the connection, runs
/// `create_schema()`, and stores the connection in the global `DB` once.
/// Subsequent calls are no-ops.
///
/// # Errors
/// - Database directory cannot be created.
/// - `rusqlite::Connection::open()` fails.
/// - `create_schema()` fails.
pub fn init_db(db_path: &Path) -> Result<()> {
    // Fast path: already initialized.
    if DB.get().is_some() {
        return Ok(());
    }

    // Ensure parent directory exists.
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    crate::vector::register_vec_on_connection(&conn)?;
    create_schema(&conn)?;
    let _ = DB.set(Mutex::new(conn));
    Ok(())
}

/// Return a reference to the initialized database connection.
///
/// # Panics
/// Panics if `init_db()` has not been called yet.
///
/// # Errors
/// Returns an error if the mutex is poisoned.
pub fn get_db() -> Result<std::sync::MutexGuard<'static, Connection>> {
    DB.get()
        .ok_or_else(|| anyhow::anyhow!("Database not initialized. Call init_db() first."))?
        .lock()
        .map_err(|e| anyhow::anyhow!("Database mutex poisoned: {}", e))
}

/// Create the database schema.
///
/// Creates the `files` / `files_fts` tables plus the `trace_*` tables
/// if they do not already exist.
///
/// # Errors
/// - Any SQL statement fails.
pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            content_hash TEXT NOT NULL,
            last_modified TEXT NOT NULL,
            content TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
            content,
            content='files',
            content_rowid='id'
        );

        CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
            INSERT INTO files_fts(rowid, content) VALUES (new.id, new.content);
        END;

        CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
            INSERT INTO files_fts(files_fts, rowid, content) VALUES('delete', old.id, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
            INSERT INTO files_fts(files_fts, rowid, content) VALUES('delete', old.id, old.content);
            INSERT INTO files_fts(rowid, content) VALUES (new.id, new.content);
        END;

        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            last_indexed_commit TEXT,
            indexed_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS trace_nodes (
            node_id TEXT PRIMARY KEY,
            node_type TEXT NOT NULL,
            title TEXT NOT NULL,
            file_path TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'active',
            category TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS trace_links (
            link_id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id TEXT NOT NULL REFERENCES trace_nodes(node_id),
            target_id TEXT NOT NULL REFERENCES trace_nodes(node_id),
            link_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'valid',
            detected_at TEXT NOT NULL,
            UNIQUE(source_id, target_id, link_type)
        );

        CREATE INDEX IF NOT EXISTS idx_trace_links_source ON trace_links(source_id);
        CREATE INDEX IF NOT EXISTS idx_trace_links_target ON trace_links(target_id);
        CREATE INDEX IF NOT EXISTS idx_trace_nodes_type ON trace_nodes(node_type);
        CREATE INDEX IF NOT EXISTS idx_trace_nodes_status ON trace_nodes(status);

        CREATE TABLE IF NOT EXISTS code_anchors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            anchor TEXT NOT NULL,
            repo_path TEXT NOT NULL,
            file_path TEXT NOT NULL,
            line_number INTEGER,
            anchor_type TEXT NOT NULL,
            target_doc TEXT,
            file_hash TEXT,
            last_seen TEXT NOT NULL,
            UNIQUE(anchor, file_path, line_number)
        );

        CREATE INDEX IF NOT EXISTS idx_code_anchors_anchor ON code_anchors(anchor);
        ",
    )?;
    // Initialize vector embedding tables
    crate::vector::init_vector(conn)?;
    Ok(())
}

/// Insert or update a code anchor entry.
#[allow(clippy::too_many_arguments)]
pub fn upsert_code_anchor(
    conn: &Connection,
    anchor: &str,
    repo_path: &str,
    file_path: &str,
    line_number: u32,
    anchor_type: &str,
    target_doc: Option<&str>,
    file_hash: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO code_anchors (anchor, repo_path, file_path, line_number, anchor_type, target_doc, file_hash, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
         ON CONFLICT(anchor, file_path, line_number) DO UPDATE SET
            file_hash = excluded.file_hash,
            last_seen = datetime('now')",
        rusqlite::params![anchor, repo_path, file_path, line_number, anchor_type, target_doc, file_hash],
    )?;
    Ok(())
}

/// Remove all code anchors for a specific project (for re-scan).
pub fn clear_project_anchors(conn: &Connection, repo_path: &str) -> Result<()> {
    conn.execute("DELETE FROM code_anchors WHERE repo_path = ?1", rusqlite::params![repo_path])?;
    Ok(())
}

/// Get count of code anchors matching a document anchor name.
pub fn count_anchors_for_doc(conn: &Connection, anchor_name: &str) -> Result<u32> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM code_anchors WHERE anchor = ?1 AND anchor_type = 'doc_anchor'",
        rusqlite::params![anchor_name],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Store or update the last indexed git commit hash.
pub fn set_last_indexed_commit(conn: &Connection, commit: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_version (version, last_indexed_commit, indexed_at)
         VALUES (1, ?1, datetime('now'))
         ON CONFLICT(version) DO UPDATE SET
            last_indexed_commit = excluded.last_indexed_commit,
            indexed_at = datetime('now')",
        rusqlite::params![commit],
    )?;
    Ok(())
}

/// Retrieve the last indexed git commit hash, if any.
pub fn get_last_indexed_commit(conn: &Connection) -> Result<Option<String>> {
    let result =
        conn.query_row("SELECT last_indexed_commit FROM schema_version WHERE version = 1", [], |row| row.get(0));
    match result {
        Ok(commit) => Ok(Some(commit)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Remove all trace data (for full rebuild).
pub fn clear_trace_graph(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM trace_links;
         DELETE FROM trace_nodes;
         DELETE FROM schema_version;",
    )?;
    Ok(())
}

/// Insert or update a trace node.
#[allow(clippy::too_many_arguments)]
pub fn upsert_trace_node(
    conn: &Connection,
    node_id: &str,
    node_type: &str,
    title: &str,
    file_path: &str,
    revision: u32,
    status: &str,
    category: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO trace_nodes (node_id, node_type, title, file_path, revision, status, category, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), datetime('now'))
         ON CONFLICT(node_id) DO UPDATE SET
            title = excluded.title,
            file_path = excluded.file_path,
            revision = excluded.revision,
            status = excluded.status,
            category = excluded.category,
            updated_at = datetime('now')",
        rusqlite::params![node_id, node_type, title, file_path, revision, status, category],
    )?;
    Ok(())
}

/// Insert a trace link between two nodes.
pub fn upsert_trace_link(conn: &Connection, source_id: &str, target_id: &str, link_type: &str) -> Result<()> {
    // Ensure target node exists (create placeholder if missing)
    conn.execute(
        "INSERT OR IGNORE INTO trace_nodes (node_id, node_type, title, file_path, revision, status, created_at, updated_at)
         VALUES (?1, 'external', ?1, '', 1, 'active', datetime('now'), datetime('now'))",
        rusqlite::params![target_id],
    )?;
    conn.execute(
        "INSERT INTO trace_links (source_id, target_id, link_type, status, detected_at)
         VALUES (?1, ?2, ?3, 'valid', datetime('now'))
         ON CONFLICT(source_id, target_id, link_type) DO UPDATE SET
            status = 'valid',
            detected_at = datetime('now')",
        rusqlite::params![source_id, target_id, link_type],
    )?;
    Ok(())
}

/// Mark a trace node as removed (soft-delete) and clean its links.
pub fn remove_trace_node(conn: &Connection, node_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE trace_nodes SET status = 'removed', updated_at = datetime('now') WHERE node_id = ?1",
        rusqlite::params![node_id],
    )?;
    conn.execute("DELETE FROM trace_links WHERE source_id = ?1 OR target_id = ?1", rusqlite::params![node_id])?;
    Ok(())
}

/// Mark stale links: links whose source node has a newer updated_at than the link.
pub fn mark_stale_links(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE trace_links SET status = 'stale'
         WHERE status = 'valid'
           AND detected_at < (
               SELECT updated_at FROM trace_nodes
               WHERE trace_nodes.node_id = trace_links.source_id
           )",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use rusqlite::params;

    #[test]
    fn test_create_schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let has_files: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='files'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(has_files, "files table should exist");

        let has_fts: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='files_fts'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(has_fts, "files_fts virtual table should exist");

        let has_trace_nodes: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='trace_nodes'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(has_trace_nodes, "trace_nodes table should exist");

        let has_trace_links: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='trace_links'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(has_trace_links, "trace_links table should exist");

        let has_schema_ver: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_schema_ver, "schema_version table should exist");
    }

    #[test]
    fn test_create_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_schema(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn test_init_db_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let fresh = DB.get().is_none();
        assert!(init_db(&db_path).is_ok());
        if fresh {
            assert!(db_path.exists());
        }
    }

    #[test]
    fn test_fts5_insert_and_query() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            params!["/test/doc.md", "abc123", "2026-07-09T12:00:00Z", "hello world test content"],
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT files.path FROM files_fts
                 JOIN files ON files_fts.rowid = files.id
                 WHERE files_fts MATCH ?1 ORDER BY rank",
            )
            .unwrap();
        let results: Vec<String> =
            stmt.query_map(params!["hello"], |row| row.get(0)).unwrap().filter_map(|r| r.ok()).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "/test/doc.md");
    }

    #[test]
    fn test_fts5_insert_and_query_no_match() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            params!["/test/doc.md", "abc123", "2026-07-09T12:00:00Z", "hello world test content"],
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT files.path FROM files_fts
                 JOIN files ON files_fts.rowid = files.id
                 WHERE files_fts MATCH ?1 ORDER BY rank",
            )
            .unwrap();
        let results: Vec<String> =
            stmt.query_map(params!["nonexistent"], |row| row.get(0)).unwrap().filter_map(|r| r.ok()).collect();
        assert!(results.is_empty());
    }

    #[test]
    fn test_upsert_trace_node() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        upsert_trace_node(
            &conn,
            "BFT-001",
            "bft",
            "Test Feature",
            "docs/01/bft-001.md",
            1,
            "active",
            Some("01-business-foundation"),
        )
        .unwrap();

        let (node_type, title): (String, String) = conn
            .query_row("SELECT node_type, title FROM trace_nodes WHERE node_id = 'BFT-001'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(node_type, "bft");
        assert_eq!(title, "Test Feature");

        upsert_trace_node(
            &conn,
            "BFT-001",
            "bft",
            "Updated Feature",
            "docs/01/bft-001.md",
            2,
            "active",
            Some("01-business-foundation"),
        )
        .unwrap();

        let updated_title: String =
            conn.query_row("SELECT title FROM trace_nodes WHERE node_id = 'BFT-001'", [], |row| row.get(0)).unwrap();
        assert_eq!(updated_title, "Updated Feature");
    }

    #[test]
    fn test_upsert_trace_link() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        upsert_trace_node(&conn, "BFT-001", "bft", "Feature", "f.md", 1, "active", None).unwrap();
        upsert_trace_node(&conn, "ADR-001", "adr", "Decision", "a.md", 1, "active", None).unwrap();
        upsert_trace_link(&conn, "BFT-001", "ADR-001", "needs").unwrap();

        let (link_type, status): (String, String) = conn
            .query_row(
                "SELECT link_type, status FROM trace_links WHERE source_id = 'BFT-001' AND target_id = 'ADR-001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(link_type, "needs");
        assert_eq!(status, "valid");
    }

    #[test]
    fn test_remove_trace_node() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        upsert_trace_node(&conn, "BFT-001", "bft", "Feature", "f.md", 1, "active", None).unwrap();
        upsert_trace_node(&conn, "ADR-001", "adr", "Decision", "a.md", 1, "active", None).unwrap();
        upsert_trace_link(&conn, "BFT-001", "ADR-001", "needs").unwrap();

        remove_trace_node(&conn, "BFT-001").unwrap();

        let status: String =
            conn.query_row("SELECT status FROM trace_nodes WHERE node_id = 'BFT-001'", [], |row| row.get(0)).unwrap();
        assert_eq!(status, "removed", "node should be marked removed");

        let link_count: u32 = conn
            .query_row("SELECT COUNT(*) FROM trace_links WHERE source_id = 'BFT-001'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(link_count, 0, "links should be cleaned");
    }

    #[test]
    fn test_set_and_get_last_indexed_commit() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        assert!(get_last_indexed_commit(&conn).unwrap().is_none(), "no commit initially");

        set_last_indexed_commit(&conn, "abc123def").unwrap();
        let commit = get_last_indexed_commit(&conn).unwrap();
        assert_eq!(commit, Some("abc123def".to_string()));

        set_last_indexed_commit(&conn, "456789").unwrap();
        let updated = get_last_indexed_commit(&conn).unwrap();
        assert_eq!(updated, Some("456789".to_string()));
    }

    #[test]
    fn test_clear_trace_graph() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        upsert_trace_node(&conn, "TST-001", "tz", "Test", "t.md", 1, "active", None).unwrap();
        set_last_indexed_commit(&conn, "abc").unwrap();

        clear_trace_graph(&conn).unwrap();

        let node_count: u32 = conn.query_row("SELECT COUNT(*) FROM trace_nodes", [], |row| row.get(0)).unwrap();
        assert_eq!(node_count, 0, "all nodes cleared");
    }
}

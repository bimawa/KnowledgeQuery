use anyhow::Result;
use rusqlite::Connection;

/// A single search result from FTS5.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Absolute or relative path to the file.
    pub path: String,
    /// Snippet of surrounding context (from FTS5 `snippet()` function).
    pub context: String,
    /// Relevance score (higher = more relevant).
    pub score: f64,
}

/// Execute an FTS5 full-text search query.
///
/// # Arguments
/// * `conn` — An open SQLite connection with FTS5 schema.
/// * `query` — FTS5 query string (supports quoted phrases, prefix `*`, etc.).
/// * `limit` — Maximum number of results to return.
///
/// # Returns
/// A vector of `SearchResult` sorted by relevance (highest first).
///
/// FTS5 `rank` is a built-in column; lower is better for BM25.
/// We expose `score` as `-rank` so higher = more relevant.
///
/// # Errors
/// - Invalid FTS5 query syntax (returns error to caller).
/// - Database query fails.
pub fn search_fts(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let sql = "SELECT files.path,
                      snippet(files_fts, 0, '<b>', '</b>', '...', 32) AS context,
                      -rank AS score
               FROM files_fts
               JOIN files ON files_fts.rowid = files.id
               WHERE files_fts MATCH ?
               ORDER BY rank
               LIMIT ?";

    let mut stmt = conn.prepare(sql)?;
    let params: [&dyn rusqlite::types::ToSql; 2] =
        [&query as &dyn rusqlite::types::ToSql, &(limit as i64) as &dyn rusqlite::types::ToSql];
    let results = stmt
        .query_map(params, |row| {
            Ok(SearchResult {
                path: row.get::<_, String>(0)?,
                context: row.get::<_, String>(1)?,
                score: row.get::<_, f64>(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;
    use rusqlite::Connection;

    fn setup_db_with_content() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        // Insert test data using individual statements to ensure triggers fire
        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["/doc1.md", "h1", "2026-01-01", "The quick brown fox jumps over the lazy dog."],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["/doc2.md", "h2", "2026-01-02", "Fox hunting is a controversial sport."],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (path, content_hash, last_modified, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["/doc3.md", "h3", "2026-01-03", "The lazy dog sleeps all day."],
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_search_fts_returns_matching_results() {
        let conn = setup_db_with_content();
        let results = search_fts(&conn, "fox", 10).unwrap();
        assert_eq!(results.len(), 2);
        // Both doc1 and doc2 contain "fox"
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"/doc1.md"));
        assert!(paths.contains(&"/doc2.md"));
    }

    #[test]
    fn test_search_fts_respects_limit() {
        let conn = setup_db_with_content();
        let results = search_fts(&conn, "fox", 1).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_fts_ordered_by_relevance() {
        let conn = setup_db_with_content();
        // Searching "lazy dog" should rank doc3 (both words) higher than doc1 (one word)
        let results = search_fts(&conn, "lazy dog", 10).unwrap();
        assert!(!results.is_empty());
        // First result should be doc3 (both terms match)
        assert_eq!(results[0].path, "/doc3.md");
    }

    #[test]
    fn test_search_fts_no_match() {
        let conn = setup_db_with_content();
        let results = search_fts(&conn, "nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_fts_result_has_context() {
        let conn = setup_db_with_content();
        let results = search_fts(&conn, "fox", 1).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].context.is_empty(), "context should not be empty");
        assert!(results[0].score > 0.0, "score should be positive");
    }

    #[test]
    fn test_search_fts_quoted_phrase() {
        let conn = setup_db_with_content();
        // Only doc1 contains the exact phrase "quick brown fox"
        let results = search_fts(&conn, "\"quick brown fox\"", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "/doc1.md");
    }

    #[test]
    fn test_search_fts_prefix_wildcard() {
        let conn = setup_db_with_content();
        // "jum*" should match "jumps" in doc1
        let results = search_fts(&conn, "jum*", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "/doc1.md");
    }
}

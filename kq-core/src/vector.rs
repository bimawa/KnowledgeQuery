use std::collections::HashMap;
use std::sync::Once;

use anyhow::{Context, Result};
use rusqlite::Connection;
use rusqlite::ffi as sqlite_ffi;
use sqlite_vec::sqlite3_vec_init;

const EMBEDDING_DIM: usize = 384;
static REGISTER_VEC_GLOBAL: Once = Once::new();

/// Register vec0 globally (for future connections) and directly on this connection.
pub fn register_vec_on_connection(db: &Connection) -> Result<()> {
    type InitFn = rusqlite::auto_extension::RawAutoExtension;
    REGISTER_VEC_GLOBAL.call_once(|| unsafe {
        sqlite_ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), InitFn>(
            sqlite3_vec_init as *const (),
        )));
    });
    // Direct registration on this connection.
    // sqlite3_vec_init in C expects (sqlite3*, char**, const sqlite3_api_routines*).
    // With SQLITE_CORE, the api_routines can be null.
    unsafe {
        let ptr: *const () = sqlite3_vec_init as *const ();
        let f: InitFn = std::mem::transmute(ptr);
        let rc = f(db.handle(), std::ptr::null_mut(), std::ptr::null());
        if rc != 0 {
            anyhow::bail!("sqlite3_vec_init failed with code {}", rc);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_id: i64,
    pub path: String,
    pub content: String,
    pub score: f64,
}

pub fn init_vector(db: &Connection) -> Result<()> {
    register_vec_on_connection(db)?;
    db.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(embedding FLOAT[{EMBEDDING_DIM}] distance_metric=cosine);
         CREATE TABLE IF NOT EXISTS vec_chunks (
             rowid INTEGER PRIMARY KEY, file_id INTEGER NOT NULL REFERENCES files(id),
             chunk_index INTEGER NOT NULL, chunk_text TEXT NOT NULL,
             content_hash TEXT NOT NULL DEFAULT ''
         );
         CREATE INDEX IF NOT EXISTS idx_vec_chunks_file ON vec_chunks(file_id);"
    )).context("Failed to create vector tables")?;
    Ok(())
}

pub fn store_embedding(
    db: &Connection,
    file_id: i64,
    chunk_index: usize,
    chunk_text: &str,
    embedding: &[f32],
) -> Result<()> {
    if embedding.len() != EMBEDDING_DIM {
        anyhow::bail!("Dimension mismatch: expected {}, got {}", EMBEDDING_DIM, embedding.len());
    }
    let blob = f32_slice_to_bytes(embedding);
    db.execute("INSERT INTO vec_embeddings (embedding) VALUES (?1)", rusqlite::params![blob])?;
    let rowid = db.last_insert_rowid();
    db.execute(
        "INSERT INTO vec_chunks (rowid, file_id, chunk_index, chunk_text, content_hash) VALUES (?1, ?2, ?3, ?4, '')",
        rusqlite::params![rowid, file_id, chunk_index as i64, chunk_text],
    )?;
    Ok(())
}

pub fn search_vector(db: &Connection, query_vector: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
    if query_vector.len() != EMBEDDING_DIM {
        anyhow::bail!("Dimension mismatch: expected {}, got {}", EMBEDDING_DIM, query_vector.len());
    }
    let mut stmt = db.prepare(
        "SELECT vec_embeddings.rowid, distance, chunk_text
         FROM vec_embeddings JOIN vec_chunks ON vec_embeddings.rowid = vec_chunks.rowid
         WHERE embedding MATCH ?1 AND k = ?2",
    )?;
    Ok(stmt
        .query_map(rusqlite::params![f32_slice_to_bytes(query_vector), limit as i64], |row| {
            Ok(SearchResult { file_id: 0, path: String::new(), content: row.get(2)?, score: row.get::<_, f64>(1)? })
        })?
        .filter_map(|r| r.ok())
        .collect())
}

pub fn hybrid_search(
    db: &Connection,
    query: &str,
    query_vector: &[f32],
    fts_limit: usize,
    vector_limit: usize,
) -> Result<Vec<SearchResult>> {
    let fts = crate::search::search_fts(db, query, fts_limit)?
        .into_iter()
        .map(|r| SearchResult { file_id: 0, path: r.path, content: r.context, score: r.score })
        .collect::<Vec<_>>();
    let vec = search_vector(db, query_vector, vector_limit)?;
    let mut merged: HashMap<String, SearchResult> = HashMap::new();
    for (w, results) in [(0.6f64, fts), (0.4, vec)] {
        for r in results {
            merged
                .entry(r.path.clone())
                .and_modify(|e| e.score = e.score.max(r.score * w))
                .or_insert(SearchResult { score: r.score * w, ..r });
        }
    }
    let mut sorted: Vec<_> = merged.into_values().collect();
    sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(sorted)
}

fn f32_slice_to_bytes(data: &[f32]) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) }.to_vec()
}

pub fn reindex_embeddings(conn: &Connection) -> Result<usize> {
    let changed: Vec<(i64, String, String)> = conn
        .prepare(
            "SELECT f.id, f.path, f.content_hash FROM files f
         LEFT JOIN vec_chunks vc ON vc.file_id = f.id AND vc.content_hash = f.content_hash
         WHERE vc.rowid IS NULL",
        )?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();
    if changed.is_empty() {
        return Ok(0);
    }
    for (fid, _, _) in &changed {
        conn.execute("DELETE FROM vec_chunks WHERE file_id = ?1", rusqlite::params![fid])?;
    }
    Ok(changed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_chunks_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE files(id INTEGER PRIMARY KEY,path TEXT,content_hash TEXT,last_modified TEXT,content TEXT)",
            [],
        )
        .ok();
        let _ = init_vector(&conn);
        let ok: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='vec_chunks'", [], |r| {
                r.get(0)
            })
            .unwrap_or(false);
        assert!(ok);
    }

    #[test]
    fn test_dimension_mismatch() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(store_embedding(&conn, 1, 0, "t", &[0.1f32; 10]).is_err());
    }
}

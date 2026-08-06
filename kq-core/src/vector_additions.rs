
/// Re-index embeddings: delete stale vec_chunks for changed files.
pub fn reindex_embeddings(conn: &Connection) -> Result<usize> {
    let changed: Vec<(i64, String, String)> = conn.prepare(
        "SELECT f.id, f.path, f.content_hash FROM files f
         LEFT JOIN vec_chunks vc ON vc.file_id = f.id AND vc.content_hash = f.content_hash
         WHERE vc.rowid IS NULL"
    )?.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    )?.filter_map(|r| r.ok()).collect();
    if changed.is_empty() { return Ok(0); }
    for (file_id, _, _) in &changed {
        conn.execute("DELETE FROM vec_chunks WHERE file_id = ?1", rusqlite::params![file_id])?;
    }
    Ok(changed.len())
}

/// Generate embeddings for all files that need them.
pub fn embed_all_files(db: &Connection, model: &kq_embeddings::EmbeddingModel) -> Result<usize> {
    let files: Vec<(i64, String, String, String)> = db.prepare(
        "SELECT f.id, f.path, f.content, f.content_hash FROM files f
         LEFT JOIN vec_chunks vc ON vc.file_id = f.id AND vc.content_hash = f.content_hash
         WHERE vc.rowid IS NULL"
    )?.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    )?.filter_map(|r| r.ok()).collect();
    if files.is_empty() { return Ok(0); }
    let mut embedded = 0usize;
    for (file_id, _path, content, content_hash) in &files {
        let bytes = content.as_bytes();
        let chunk_byte_size = 2048;
        let mut offset = 0usize;
        let mut chunk_idx = 0usize;
        while offset < bytes.len() {
            let end = (offset + chunk_byte_size).min(bytes.len());
            if let Ok(chunk) = std::str::from_utf8(&bytes[offset..end]) {
                if let Ok(emb) = model.embed(chunk) {
                    store_embedding(db, *file_id, chunk_idx, chunk, &emb)?;
                    embedded += 1;
                    chunk_idx += 1;
                }
            }
            offset += chunk_byte_size.saturating_sub(256);
        }
        if !content_hash.is_empty() {
            db.execute("UPDATE vec_chunks SET content_hash = ?1 WHERE file_id = ?2",
                rusqlite::params![content_hash, file_id])?;
        }
    }
    Ok(embedded)
}

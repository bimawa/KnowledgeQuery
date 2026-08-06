pub fn register_vec_on_connection(db: &Connection) -> Result<()> {
    REGISTER_VEC.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), _>(
            sqlite3_vec_init as *const (),
        )));
    });
    unsafe {
        type F = unsafe extern "C" fn(*mut rusqlite::ffi::sqlite3, *mut *mut std::os::raw::c_char, *const std::ffi::c_void) -> i32;
        let ptr: *const () = sqlite3_vec_init as *const ();
        let f: F = std::mem::transmute(ptr);
        let rc = f(db.handle(), std::ptr::null_mut(), std::ptr::null());
        if rc != 0 { anyhow::bail!("vec0 init failed: {}", rc); }
    }
    Ok(())
}

pub fn reindex_embeddings(conn: &Connection) -> Result<usize> {
    let changed: Vec<(i64, String, String)> = conn.prepare(
        "SELECT f.id, f.path, f.content_hash FROM files f
         LEFT JOIN vec_chunks vc ON vc.file_id = f.id AND vc.content_hash = f.content_hash
         WHERE vc.rowid IS NULL"
    )?.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.filter_map(|r| r.ok()).collect();
    if changed.is_empty() { return Ok(0); }
    for (fid, _, _) in &changed { conn.execute("DELETE FROM vec_chunks WHERE file_id = ?1", rusqlite::params![fid])?; }
    Ok(changed.len())
}

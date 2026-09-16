use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct ResyncReport {
    pub indexed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub nodes: usize,
    pub links: usize,
    pub pruned_files: usize,
    pub pruned_nodes: usize,
}

pub fn is_knowledge_repo(dir: &Path) -> bool {
    dir.join("knowledge.toml").exists()
}

pub fn resync_files(dir: &Path) -> Result<ResyncReport> {
    let conn = crate::db::get_db()?;
    let mut report = ResyncReport::default();
    let index_report = crate::indexer::index_all_internal(&conn, dir)?;
    report.indexed = index_report.indexed;
    report.skipped = index_report.skipped;
    report.failed = index_report.failed;
    report.pruned_files = prune_missing_files(&conn, dir)?;
    Ok(report)
}

pub fn resync_repo(repo_path: &Path) -> Result<ResyncReport> {
    let mut report = resync_files(repo_path)?;
    let conn = crate::db::get_db()?;

    let (nodes, links, pruned_nodes) = rebuild_trace_from_tree(&conn, repo_path)?;
    report.nodes = nodes;
    report.links = links;
    report.pruned_nodes = pruned_nodes;

    crate::db::mark_stale_links(&conn)?;

    if let Ok(repo) = crate::git::open_repo(repo_path)
        && let Ok(head) = repo.head()
        && let Ok(commit) = head.peel_to_commit()
    {
        let _ = crate::db::set_last_indexed_commit(&conn, &commit.id().to_string());
    }

    Ok(report)
}

fn prune_missing_files(conn: &rusqlite::Connection, repo_path: &Path) -> Result<usize> {
    let mut live: HashSet<String> = HashSet::new();
    for entry in WalkDir::new(repo_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if (ext == "md" || ext == "tsp") && path.is_file() {
            live.insert(path.to_string_lossy().to_string());
        }
    }

    let stored: Vec<(i64, String)> = conn
        .prepare("SELECT id, path FROM files")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut pruned = 0;
    for (id, path) in &stored {
        if !live.contains(path) && !Path::new(path).exists() {
            conn.execute("DELETE FROM files WHERE id = ?1", rusqlite::params![id])?;
            pruned += 1;
        }
    }
    Ok(pruned)
}

fn rebuild_trace_from_tree(conn: &rusqlite::Connection, repo_path: &Path) -> Result<(usize, usize, usize)> {
    let doc_nodes = crate::docs::list_doc_nodes(repo_path).unwrap_or_default();
    let models = crate::typespec::list_types(repo_path).unwrap_or_default();

    let mut current_ids: HashSet<String> = HashSet::new();
    let mut referenced: HashSet<String> = HashSet::new();

    for node in &doc_nodes {
        crate::db::upsert_trace_node(
            conn,
            &node.id,
            &node.doc_type.to_string(),
            &node.title,
            &node.file_path,
            node.revision,
            "active",
            node.category.as_deref(),
        )?;
        current_ids.insert(node.id.clone());
        for t in node.needs.iter().chain(&node.covers).chain(&node.inline_refs) {
            referenced.insert(t.clone());
        }
    }

    for model in &models {
        crate::db::upsert_trace_node(
            conn,
            &model.name,
            "typespec",
            &model.name,
            &format!("TypeSpec/{}", model.file),
            1,
            "active",
            Some("TypeSpec"),
        )?;
        current_ids.insert(model.name.clone());
        for t in &model.doc_refs {
            referenced.insert(t.clone());
        }
    }

    conn.execute_batch("DELETE FROM trace_links;")?;

    let mut links = 0;
    for node in &doc_nodes {
        for t in node.needs.iter() {
            crate::db::upsert_trace_link(conn, &node.id, t, "needs")?;
            links += 1;
        }
        for t in node.covers.iter() {
            crate::db::upsert_trace_link(conn, &node.id, t, "covers")?;
            links += 1;
        }
        for t in node.inline_refs.iter() {
            crate::db::upsert_trace_link(conn, &node.id, t, "references")?;
            links += 1;
        }
    }
    for model in &models {
        for t in &model.doc_refs {
            crate::db::upsert_trace_link(conn, &model.name, t, "covers")?;
            links += 1;
        }
    }

    let stored_nodes: Vec<(String, String)> = conn
        .prepare("SELECT node_id, node_type FROM trace_nodes")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut pruned_nodes = 0;
    for (node_id, _node_type) in &stored_nodes {
        if current_ids.contains(node_id) {
            continue;
        }
        if referenced.contains(node_id) {
            conn.execute(
                "UPDATE trace_nodes SET node_type = 'external', status = 'active', file_path = '', updated_at = datetime('now') WHERE node_id = ?1",
                rusqlite::params![node_id],
            )?;
        } else {
            crate::db::remove_trace_node(conn, node_id)?;
            pruned_nodes += 1;
        }
    }

    conn.execute(
        "DELETE FROM trace_nodes WHERE node_type = 'external'
         AND node_id NOT IN (SELECT target_id FROM trace_links)",
        [],
    )?;

    Ok((current_ids.len(), links, pruned_nodes))
}

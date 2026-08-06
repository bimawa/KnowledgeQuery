use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::docs;
use crate::typespec;

#[derive(Debug, Clone)]
pub struct Link {
    pub source: String,
    pub refs: Vec<String>,
}

#[derive(Debug)]
pub struct TraceReport {
    pub complete_chains: Vec<Link>,
    pub orphans: Vec<String>,
    pub broken_links: Vec<String>,
    pub dangling_docs: Vec<String>,
}

pub fn traceability(path: &Path) -> Result<TraceReport> {
    let models = typespec::list_types(path)?;
    let doc_files = docs::list_docs(path)?;

    // Collect all @doc IDs referenced from docs/**/*.md
    let mut doc_refs_in_md: HashSet<String> = HashSet::new();
    let mut doc_files_with_refs: Vec<&str> = Vec::new();

    for (_category, filepath) in &doc_files {
        let content = fs::read_to_string(filepath).with_context(|| format!("Failed to read {}", filepath))?;

        let mut found_in_file = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(pos) = trimmed.find("@doc ") {
                let after = &trimmed[pos + 5..];
                let id: String = after.chars().take_while(|c| *c != ' ' && *c != '\t').collect();
                if !id.is_empty() {
                    doc_refs_in_md.insert(id.clone());
                    found_in_file = true;
                }
            }
        }
        if found_in_file {
            doc_files_with_refs.push(filepath);
        }
    }

    // Collect all doc IDs referenced from TypeSpec models
    let mut model_refs: HashSet<String> = HashSet::new();
    for model in &models {
        for r in &model.doc_refs {
            model_refs.insert(r.clone());
        }
    }

    let _all_refs: HashSet<String> = doc_refs_in_md.union(&model_refs).cloned().collect();

    // Build bidirectional graph
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // TypeSpec model → doc IDs
    for model in &models {
        if !model.doc_refs.is_empty() {
            graph.entry(format!("TypeSpec/{}", model.file)).or_default().extend(model.doc_refs.iter().cloned());
        }
    }

    // doc file → doc IDs
    for (_category, filepath) in &doc_files {
        let content = fs::read_to_string(filepath)?;
        let mut refs_in_file: Vec<String> = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(pos) = trimmed.find("@doc ") {
                let after = &trimmed[pos + 5..];
                let id: String = after.chars().take_while(|c| *c != ' ' && *c != '\t').collect();
                if !id.is_empty() {
                    refs_in_file.push(id);
                }
            }
        }
        if !refs_in_file.is_empty() {
            graph.entry(filepath.clone()).or_default().extend(refs_in_file);
        }
    }

    // 4. Orphans: models with no back-links from docs
    let orphans: Vec<String> = models
        .iter()
        .filter(|m| !m.doc_refs.is_empty())
        .filter(|m| m.doc_refs.iter().all(|r| !doc_refs_in_md.contains(r.as_str())))
        .map(|m| m.name.clone())
        .collect();

    // 5. Broken links: @doc ID referenced from models but no file contains that ID
    let broken_links: Vec<String> = models
        .iter()
        .flat_map(|m| &m.doc_refs)
        .filter(|r| !doc_refs_in_md.contains(r.as_str()))
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // 6. Dangling docs: .md files with zero @doc references
    let dangling_docs: Vec<String> = doc_files
        .iter()
        .filter(|(_, fp)| !doc_files_with_refs.contains(&fp.as_str()))
        .map(|(_, fp)| fp.clone())
        .collect();

    // 7. Complete chains: bidirectional links where model → doc AND doc → model
    let complete_chains: Vec<Link> =
        graph.into_iter().filter(|(_, refs)| !refs.is_empty()).map(|(source, refs)| Link { source, refs }).collect();

    // 8. Print table
    print_report(&models, &orphans, &broken_links, &dangling_docs, &complete_chains);

    Ok(TraceReport { complete_chains, orphans, broken_links, dangling_docs })
}

pub fn orphans(path: &Path) -> Result<Vec<String>> {
    let report = traceability(path)?;
    Ok(report.orphans)
}

fn print_report(
    models: &[typespec::TypeModel],
    orphans: &[String],
    broken_links: &[String],
    dangling_docs: &[String],
    complete_chains: &[Link],
) {
    let mut table = String::new();
    let _ = writeln!(table, "| Object | Type | Status | Links | Recommendation |");
    let _ = writeln!(table, "|--------|------|--------|-------|----------------|");

    let orphan_set: HashSet<&str> = orphans.iter().map(String::as_str).collect();
    let _broken_set: HashSet<&str> = broken_links.iter().map(String::as_str).collect();
    let dangling_set: HashSet<&str> = dangling_docs.iter().map(String::as_str).collect();
    let _chain_sources: HashSet<&str> = complete_chains.iter().map(|c| c.source.as_str()).collect();

    for model in models {
        if model.doc_refs.is_empty() {
            let _ = writeln!(table, "| {} | TypeSpec model | No @doc refs | — | Add @doc annotations |", model.name);
        } else if orphan_set.contains(model.name.as_str()) {
            let _ = writeln!(
                table,
                "| {} | TypeSpec model | Orphan | {} | Link from docs or remove @doc |",
                model.name,
                model.doc_refs.join(", ")
            );
        } else {
            let _ = writeln!(table, "| {} | TypeSpec model | Traced | {} | ✓ |", model.name, model.doc_refs.join(", "));
        }
    }

    for broken in broken_links {
        let _ = writeln!(table, "| {} | Broken @doc | Missing | — | Create doc or fix reference |", broken);
    }

    for dangling in dangling_docs {
        let _ = writeln!(table, "| {} | Dangling doc | No @doc refs | — | Add @doc annotations |", dangling);
    }

    for chain in complete_chains {
        if !dangling_set.contains(chain.source.as_str()) {
            let _ = writeln!(table, "| {} | Source | Linked | {} | ✓ |", chain.source, chain.refs.join(", "));
        }
    }

    print!("{}", table);
}

/// Chain definition for deep coverage.
const DEFAULT_CHAIN: &[(&str, &str)] = &[
    ("bft", "brd"),
    ("bft", "frd"),
    ("bft", "nfr"),
    ("brd", "adr"),
    ("frd", "adr"),
    ("nfr", "adr"),
    ("adr", "tz"),
    ("tz", "typespec"),
];

const TERMINATING_TYPES: &[&str] = &["typespec", "idea", "glossary"];

/// A single deep coverage result for one node.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeepCoverageResult {
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    pub chain_status: String, // covered | partial | broken | orphan
    pub coverage_pct: f64,
    pub missing_links: Vec<String>,
}

/// Run deep coverage traceability.
///
/// - `deep`: if true, prints deep coverage chains instead of basic report
/// - `chain`: optional custom chain, e.g. "bft adr tz"
/// - `json`: if true, returns JSON string instead of printing table
pub fn traceability_deep(path: &Path, deep: bool, chain: Option<&str>, json: bool) -> Result<String> {
    // Ensure trace graph is indexed
    rebuild_trace_graph(path)?;

    let db = crate::db::get_db()?;

    // Parse custom chain or use default
    let chain_def: Vec<(&str, &str)> = if let Some(c) = chain {
        let types: Vec<&str> = c.split_whitespace().collect();
        types.windows(2).map(|w| (w[0], w[1])).collect()
    } else {
        DEFAULT_CHAIN.to_vec()
    };

    // Get all active nodes
    let mut stmt = db.prepare(
        "SELECT node_id, node_type, title FROM trace_nodes WHERE status = 'active' ORDER BY node_type, node_id",
    )?;
    let nodes: Vec<(String, String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.filter_map(|r| r.ok()).collect();

    let mut results: Vec<DeepCoverageResult> = Vec::new();

    for (node_id, node_type, title) in &nodes {
        if TERMINATING_TYPES.contains(&node_type.as_str()) {
            results.push(DeepCoverageResult {
                node_id: node_id.clone(),
                node_type: node_type.clone(),
                title: title.clone(),
                chain_status: "covered".to_string(),
                coverage_pct: 100.0,
                missing_links: vec![],
            });
            continue;
        }

        // Find which chain edges apply from this node type
        let outgoing: Vec<&str> =
            chain_def.iter().filter(|(src, _)| *src == node_type.as_str()).map(|(_, dst)| *dst).collect();

        if outgoing.is_empty() && !deep {
            continue; // Skip nodes not in chain for basic mode
        }

        let mut missing = Vec::new();
        for target_type in &outgoing {
            // Check: any node of target_type covers this source (reverse direction)
            let has_covers: bool = db.query_row(
                "SELECT COUNT(*) > 0 FROM trace_links l
                 JOIN trace_nodes t ON t.node_id = l.source_id
                 WHERE l.target_id = ?1 AND t.node_type = ?2 AND l.status = 'valid'",
                rusqlite::params![node_id, target_type],
                |row| row.get(0),
            )?;

            // Also check: this node has an outgoing needs link to a node of target_type
            let has_needs: bool = if !has_covers {
                db.query_row(
                    "SELECT COUNT(*) > 0 FROM trace_links l
                     JOIN trace_nodes t ON t.node_id = l.target_id
                     WHERE l.source_id = ?1 AND t.node_type = ?2 AND l.status = 'valid'",
                    rusqlite::params![node_id, target_type],
                    |row| row.get(0),
                )?
            } else {
                true
            };

            if !has_covers && !has_needs {
                missing.push(format!("{}→{}", node_type, target_type));
            }
        }

        // Check incoming links (orphan detection)
        let has_incoming: bool = db.query_row(
            "SELECT COUNT(*) > 0 FROM trace_links WHERE target_id = ?1 AND status = 'valid'",
            rusqlite::params![node_id],
            |row| row.get(0),
        )?;

        let status = if !has_incoming {
            "orphan"
        } else if missing.is_empty() {
            "covered"
        } else if missing.len() < outgoing.len() {
            "partial"
        } else {
            "broken"
        };

        let total_edges = outgoing.len();
        let remaining = total_edges.saturating_sub(missing.len());
        let coverage_pct = if total_edges > 0 { remaining as f64 / total_edges as f64 * 100.0 } else { 100.0 };

        results.push(DeepCoverageResult {
            node_id: node_id.clone(),
            node_type: node_type.clone(),
            title: title.clone(),
            chain_status: status.to_string(),
            coverage_pct,
            missing_links: missing,
        });
    }

    if json {
        return Ok(serde_json::to_string_pretty(&results)?);
    }

    // Print table
    println!("\n┌─────────────────────────────────────────────────────────────────┐");
    println!("│ Deep Coverage Traceability                                    │");
    println!("├──────────┬──────────┬──────────┬──────────┬────────────────────┤");
    println!("│ Node ID  │ Type     │ Status   │ Coverage │ Missing Links      │");
    println!("├──────────┼──────────┼──────────┼──────────┼────────────────────┤");
    for r in &results {
        let status_icon = match r.chain_status.as_str() {
            "covered" => "✅ covered",
            "partial" => "⚠️ partial",
            "broken" => "❌ broken",
            "orphan" => "👻 orphan",
            _ => &r.chain_status,
        };
        let missing_str = if r.missing_links.is_empty() { "—".to_string() } else { r.missing_links.join(", ") };
        println!(
            "│ {:<8} │ {:<8} │ {:<8} │ {:>5.0}%   │ {:<18} │",
            r.node_id, r.node_type, status_icon, r.coverage_pct, missing_str
        );
    }
    println!("└──────────┴──────────┴──────────┴──────────┴────────────────────┘");

    Ok(String::new())
}

/// Rebuild the trace graph from knowledge repo documents.
///
/// Uses incremental re-index: only scans changed files since last run.
/// Falls back to full rebuild if no prior index exists.
pub fn rebuild_trace_graph(path: &Path) -> Result<()> {
    let repo = crate::git::open_repo(path)?;
    let head_commit = get_head_commit(&repo)?;

    let db = crate::db::get_db()?;
    let last_commit = crate::db::get_last_indexed_commit(&db)?;

    if last_commit.as_deref() == Some(&head_commit) {
        return Ok(()); // No changes since last index
    }

    if let Some(last) = last_commit {
        // Incremental: diff since last indexed commit
        let diff_files = get_changed_files(&repo, &last, &head_commit)?;
        for file in &diff_files {
            let file_path = path.join(&file.path);
            match file.status {
                git2::Delta::Added | git2::Delta::Modified => {
                    if file_path.extension().is_some_and(|e| e == "md")
                        && let Ok(node) = docs::parse_doc_node(&file_path)
                    {
                        crate::db::upsert_trace_node(
                            &db,
                            &node.id,
                            &node.doc_type.to_string(),
                            &node.title,
                            &node.file_path,
                            node.revision,
                            "active",
                            node.category.as_deref(),
                        )?;
                        for target in &node.needs {
                            crate::db::upsert_trace_link(&db, &node.id, target, "needs")?;
                        }
                        for target in &node.covers {
                            crate::db::upsert_trace_link(&db, &node.id, target, "covers")?;
                        }
                        for target in &node.inline_refs {
                            crate::db::upsert_trace_link(&db, &node.id, target, "references")?;
                        }
                    } else if file_path.extension().is_some_and(|e| e == "tsp") {
                        // TypeSpec changed: re-index all models (upsert + relink)
                        let models = typespec::list_types(path)?;
                        for model in &models {
                            crate::db::upsert_trace_node(
                                &db,
                                &model.name,
                                "typespec",
                                &model.name,
                                &format!("TypeSpec/{}", model.file),
                                1,
                                "active",
                                Some("TypeSpec"),
                            )?;
                            for target in &model.doc_refs {
                                crate::db::upsert_trace_link(&db, &model.name, target, "covers")?;
                            }
                        }
                    }
                }
                git2::Delta::Deleted => {
                    // Determine node_id from filename
                    if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                        crate::db::remove_trace_node(&db, stem)?;
                    }
                }
                _ => {}
            }
        }
    } else {
        // Full rebuild: scan all docs
        let nodes = docs::list_doc_nodes(path)?;
        for node in &nodes {
            crate::db::upsert_trace_node(
                &db,
                &node.id,
                &node.doc_type.to_string(),
                &node.title,
                &node.file_path,
                node.revision,
                "active",
                node.category.as_deref(),
            )?;
            for target in &node.needs {
                crate::db::upsert_trace_link(&db, &node.id, target, "needs")?;
            }
            for target in &node.covers {
                crate::db::upsert_trace_link(&db, &node.id, target, "covers")?;
            }
            for target in &node.inline_refs {
                crate::db::upsert_trace_link(&db, &node.id, target, "references")?;
            }
        }

        // Index TypeSpec models as terminating trace nodes, linked to their docs
        let models = typespec::list_types(path)?;
        for model in &models {
            crate::db::upsert_trace_node(
                &db,
                &model.name,
                "typespec",
                &model.name,
                &format!("TypeSpec/{}", model.file),
                1,
                "active",
                Some("TypeSpec"),
            )?;
            for target in &model.doc_refs {
                crate::db::upsert_trace_link(&db, &model.name, target, "covers")?;
            }
        }
    }

    // Mark stale links
    crate::db::mark_stale_links(&db)?;

    // Store last indexed commit
    crate::db::set_last_indexed_commit(&db, &head_commit)?;

    Ok(())
}

/// Scan all projects from config for code anchors.
pub fn scan_projects(path: &Path) -> Result<()> {
    let config_path = path.join("knowledge.toml");
    let cfg = kq_config::KnowledgeConfig::load(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path.display()))?;

    if cfg.projects.is_empty() {
        println!("  No projects configured in knowledge.toml.");
        return Ok(());
    }

    for project in &cfg.projects {
        let label = project.label.as_deref().unwrap_or("(unnamed)");
        println!("  Scanning: {label} ({})", project.path.display());

        let scan_patterns = project.effective_scan_patterns();
        let ignore = &cfg.watcher.ignore_patterns;

        let summary = crate::code_anchor::scan_project(&project.path, scan_patterns, ignore)?;

        println!(
            "    Files: {}, anchors: {}, @see refs: {}",
            summary.total_files, summary.total_anchors, summary.total_see_refs
        );
    }

    Ok(())
}

/// Print stale links section to stdout.
pub fn print_stale_links() -> Result<()> {
    let db = crate::db::get_db()?;
    let mut stmt = db.prepare(
        "SELECT l.source_id, l.target_id, l.link_type, n.updated_at, l.detected_at
         FROM trace_links l
         JOIN trace_nodes n ON n.node_id = l.source_id
         WHERE l.status = 'stale'
         ORDER BY n.updated_at DESC",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        return Ok(());
    }

    println!("\n  🕸️  Stale Links (source changed after link was detected):");
    for (src, tgt, ltype, updated, detected) in &rows {
        println!("    {src} --[{ltype}]--> {tgt}");
        println!("      doc updated: {updated}, link detected: {detected}");
    }

    Ok(())
}

/// Get the HEAD commit hash as a hex string.
fn get_head_commit(repo: &git2::Repository) -> Result<String> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

/// A changed file entry from git diff.
struct ChangedFile {
    path: String,
    status: git2::Delta,
}

/// Get list of changed files between two commits.
fn get_changed_files(repo: &git2::Repository, from: &str, to: &str) -> Result<Vec<ChangedFile>> {
    let from_oid = git2::Oid::from_str(from)?;
    let to_oid = git2::Oid::from_str(to)?;
    let from_commit = repo.find_commit(from_oid)?;
    let to_commit = repo.find_commit(to_oid)?;
    let from_tree = from_commit.tree()?;
    let to_tree = to_commit.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?;

    let mut files = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path() {
                files.push(ChangedFile { path: path.to_string_lossy().to_string(), status: delta.status() });
            }
            true
        },
        None,
        None,
        None,
    )?;

    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(files)
}

/// Create a task for an orphan code anchor.
pub fn create_orphan_task(anchor_name: &str, file_path: &str, repo_path: &str) -> Result<()> {
    let title = format!("Create TypeSpec + doc for {anchor_name}");
    let body = format!(
        "\n@dev Найден `@doc-anchor {anchor_name}` в:\n  {repo_path}/{file_path}\n\n\
         Что нужно:\n\
         - [ ] Создать TypeSpec модель: `kqs typespec new {anchor_name}`\n\
         - [ ] Создать TZ-документ: `kqs doc new tz \"{anchor_name}\"`\n\
         - [ ] Проверить trace: `kqs check traceability`\n"
    );

    let task = crate::task::task_new(&title, crate::task::Status::Todo, crate::task::Priority::P1, "")?;

    let repo = kq_config::repo_path(None)?;
    let task_path = repo.join("tasks").join(format!("{}.md", task.id));
    if task_path.exists() {
        let mut content = std::fs::read_to_string(&task_path)?;
        content.push_str(&body);
        std::fs::write(&task_path, content)?;
    }

    println!("  📋 Created task {}: {title}", task.id);
    Ok(())
}

/// Print stale link notifications, optionally filtered by time.
pub fn print_stale_notifications(since: Option<&str>) -> Result<()> {
    let db = crate::db::get_db()?;

    let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(s) = since {
        // since = "7d" → offset = "-7 days"
        let days = s.trim_end_matches('d').parse::<i32>().unwrap_or(7);
        let offset = format!("-{days} days");
        (
            "SELECT l.source_id, l.target_id, l.link_type, n.title, n.updated_at, l.detected_at
             FROM trace_links l
             JOIN trace_nodes n ON n.node_id = l.source_id
             WHERE l.status = 'stale'
               AND n.updated_at >= datetime('now', ?1)
             ORDER BY n.updated_at DESC"
                .to_string(),
            vec![Box::new(offset) as Box<dyn rusqlite::types::ToSql>],
        )
    } else {
        (
            "SELECT l.source_id, l.target_id, l.link_type, n.title, n.updated_at, l.detected_at
             FROM trace_links l
             JOIN trace_nodes n ON n.node_id = l.source_id
             WHERE l.status = 'stale'
             ORDER BY n.updated_at DESC"
                .to_string(),
            vec![],
        )
    };

    let mut stmt = db.prepare(&query)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows: Vec<(String, String, String, String, String, String)> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("  ✅ No stale links found.");
        return Ok(());
    }

    println!("\n  🕸️  Stale Links (doc changed, code may not match):");
    for (src, tgt, ltype, title, updated, detected) in &rows {
        println!("    {src} --[{ltype}]--> {tgt}");
        println!("      Doc: {title}");
        println!("      Updated: {updated}, Link detected: {detected}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_empty_project() {
        let dir = tempdir().unwrap();
        let report = traceability(dir.path()).unwrap();
        assert!(report.orphans.is_empty());
        assert!(report.broken_links.is_empty());
        assert!(report.dangling_docs.is_empty());
    }

    #[test]
    fn test_orphans_detection() {
        let dir = tempdir().unwrap();

        // Create TypeSpec with @doc ref
        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("user.tsp"), "// @doc USR-001\nmodel User {\n  name: string\n}").unwrap();

        // Create docs with a different @doc ref
        let doc_dir = dir.path().join("docs").join("01-business-foundation");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("architecture.md"), "# Architecture\n\n// @doc ARCH-001\n").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(report.orphans.contains(&"User".to_string()));
    }

    #[test]
    fn test_dangling_docs_detection() {
        let dir = tempdir().unwrap();

        // Create doc with no @doc refs
        let doc_dir = dir.path().join("docs").join("01-business-foundation");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("standalone.md"), "# Standalone\n\nThis doc has no TypeSpec references.").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(!report.dangling_docs.is_empty());
        assert!(report.dangling_docs.iter().any(|d| d.contains("standalone.md")));
    }

    #[test]
    fn test_broken_links_detection() {
        let dir = tempdir().unwrap();

        // TypeSpec references non-existent doc
        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("model.tsp"), "// @doc MISSING-999\nmodel Thing {}").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(report.broken_links.contains(&"MISSING-999".to_string()));
    }

    #[test]
    fn test_complete_chain() {
        let dir = tempdir().unwrap();

        // TypeSpec with @doc ref
        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("product.tsp"), "// @doc PROD-001\nmodel Product {}").unwrap();

        // Doc referencing same ID
        let doc_dir = dir.path().join("docs").join("03-architecture");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("product-spec.md"), "# Product Spec\n\n// @doc PROD-001\n").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(report.orphans.is_empty());
        assert!(report.broken_links.is_empty());
        assert!(!report.complete_chains.is_empty());
    }

    #[test]
    fn test_orphans_function() {
        let dir = tempdir().unwrap();

        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("lonely.tsp"), "// @doc LONELY-001\nmodel Lonely {}").unwrap();

        let orphans_list = orphans(dir.path()).unwrap();
        assert!(orphans_list.contains(&"Lonely".to_string()));
    }

    #[test]
    fn test_no_typespec_dir() {
        let dir = tempdir().unwrap();

        let doc_dir = dir.path().join("docs").join("01-business-foundation");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("notes.md"), "# Notes\n\nNo refs here.").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(report.orphans.is_empty());
        assert!(report.broken_links.is_empty());
        assert!(!report.dangling_docs.is_empty());
    }

    #[test]
    fn test_multiple_refs_per_model() {
        let dir = tempdir().unwrap();

        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("complex.tsp"), "// @doc C-001\n// @doc C-002\n// @doc C-003\nmodel Complex {}").unwrap();

        // Only C-001 exists in docs
        let doc_dir = dir.path().join("docs").join("04-technical-design");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("complex.md"), "# Complex\n\n// @doc C-001\n").unwrap();

        let report = traceability(dir.path()).unwrap();
        // C-002 and C-003 are broken links
        assert!(report.broken_links.contains(&"C-002".to_string()));
        assert!(report.broken_links.contains(&"C-003".to_string()));
        // Complex is NOT orphan since C-001 is back-linked (only missing refs cause orphans)
        assert!(!report.orphans.contains(&"Complex".to_string()));
    }

    #[test]
    fn test_report_table_output() {
        let dir = tempdir().unwrap();

        let ts_dir = dir.path().join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("item.tsp"), "// @doc ITEM-001\nmodel Item {}").unwrap();

        let doc_dir = dir.path().join("docs").join("02-product-ux");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("item-spec.md"), "# Item Spec\n\n// @doc ITEM-001\n").unwrap();

        let report = traceability(dir.path()).unwrap();
        assert!(!report.complete_chains.is_empty());
        // Table should have been printed without panicking
    }
}

use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::docs;
use crate::task::task_list;

const MARKER_START: &str = "<!-- kq:start -->";
const MARKER_END: &str = "<!-- kq:end -->";

pub fn generate(repo_path: &Path) -> Result<()> {
    let readme_path = repo_path.join("README.md");
    let content =
        fs::read_to_string(&readme_path).with_context(|| format!("Failed to read {}", readme_path.display()))?;
    let mut output = String::with_capacity(content.len() + 4096);
    generate_impl(repo_path, &content, &mut output)?;

    if output != content {
        fs::write(&readme_path, output).with_context(|| format!("Failed to write {}", readme_path.display()))?;
    }

    Ok(())
}

pub fn generate_to(repo_path: &Path, writer: &mut dyn Write) -> Result<()> {
    let readme_path = repo_path.join("README.md");
    let content =
        fs::read_to_string(&readme_path).with_context(|| format!("Failed to read {}", readme_path.display()))?;
    generate_impl(repo_path, &content, writer)
}

fn generate_impl(repo_path: &Path, content: &str, writer: &mut dyn Write) -> Result<()> {
    let start = content.find(MARKER_START);
    let end = content.find(MARKER_END);

    let generated = build_kanban()? + "\n\n" + &build_doc_toc(repo_path)?;

    match (start, end) {
        (Some(s), Some(e)) if e > s => {
            let before = &content[..s + MARKER_START.len()];
            let after = &content[e..];
            write!(writer, "{before}\n{generated}\n{after}")?;
        }
        _ => {
            write!(writer, "{content}\n{MARKER_START}\n{generated}\n{MARKER_END}\n")?;
        }
    }
    Ok(())
}

/// Build a table-of-contents of all documents with per-category tables.
fn build_doc_toc(repo_path: &Path) -> Result<String> {
    let doc_files = docs::list_docs(repo_path)?;
    if doc_files.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::with_capacity(2048);
    writeln!(out, "## Table of Contents")?;
    writeln!(out)?;

    let mut current_category = String::new();
    let mut cat_count = 0usize;

    for (cat, filepath) in &doc_files {
        let path = Path::new(filepath);
        let rel_str = filepath
            .strip_prefix(repo_path.to_string_lossy().as_ref())
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(filepath)
            .to_string();

        match docs::parse_doc_node(path) {
            Ok(node) => {
                if *cat != current_category {
                    current_category = cat.clone();
                    cat_count += 1;
                    if cat_count > 1 {
                        writeln!(out)?;
                    }
                    let cat_label = docs::category_label(cat);
                    writeln!(out, "**📁 {cat_label}**")?;
                    writeln!(out, "| ID | Title | Status |")?;
                    writeln!(out, "|----|-------|--------|")?;
                }
                let status_icon = match node.status.as_str() {
                    "Approved" | "Accepted" => "✅",
                    "Proposed" | "Draft" => "🔄",
                    "Deprecated" => "🗑️",
                    _ => "📄",
                };
                writeln!(
                    out,
                    "| [{id}]({path}) | {title} | {status_icon} {status} |",
                    id = node.id,
                    path = rel_str,
                    title = node.title,
                    status_icon = status_icon,
                    status = node.status,
                )?;
            }
            Err(_) => {
                let fname = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
                if *cat != current_category {
                    current_category = cat.clone();
                    cat_count += 1;
                    if cat_count > 1 {
                        writeln!(out)?;
                    }
                    let cat_label = docs::category_label(cat);
                    writeln!(out, "**📁 {cat_label}**")?;
                    writeln!(out, "| ID | Title | Status |")?;
                    writeln!(out, "|----|-------|--------|")?;
                }
                writeln!(out, "| [{fname}]({rel_str}) | — | 📄 |")?;
            }
        }
    }

    Ok(out)
}

fn build_kanban() -> Result<String> {
    let tasks = task_list(None, None)?;
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    let columns = ["todo", "in_progress", "review", "done"];
    let labels = ["Todo", "In Progress", "Review", "Done"];

    for status in &columns {
        grouped.insert(status.to_string(), Vec::new());
    }

    for task in &tasks {
        let key = task.status.to_string();
        let task_link = format!("[{}](tasks/{}.md)", task.title, task.id);
        let entry = if task.assignee.is_empty() {
            task_link
        } else {
            format!("{} ([@{}](users/{}.md))", task_link, task.assignee, task.assignee)
        };
        grouped.entry(key).or_default().push(entry);
    }

    let counts: Vec<usize> = columns.iter().map(|c| grouped[*c].len()).collect();
    let total: usize = counts.iter().sum();
    let max_rows = *counts.iter().max().unwrap_or(&0);

    let mut out = String::with_capacity(2048);
    writeln!(out, "## Kanban Board")?;
    writeln!(out)?;
    writeln!(out, "**Total: {total} tasks**")?;
    writeln!(out)?;

    for label in &labels {
        write!(out, "| {label} ")?;
    }
    writeln!(out, "|")?;

    for _ in &labels {
        write!(out, "| --- ")?;
    }
    writeln!(out, "|")?;

    for row in 0..max_rows {
        for col in &columns {
            let items = &grouped[*col];
            if row < items.len() {
                write!(out, "| {} ", items[row])?;
            } else {
                write!(out, "| — ")?;
            }
        }
        writeln!(out, "|")?;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_replacement() {
        let input = "Hello\n<!-- kq:start -->\nold\n<!-- kq:end -->\nWorld";
        let mut out = String::new();
        let tmp = std::env::temp_dir();
        let repo = tmp.join("kq_readme_test");
        let _ = std::fs::create_dir_all(&repo.join("docs/01-business-foundation"));
        let _ = std::fs::write(
            repo.join("docs/01-business-foundation/bft-001-test.md"),
            "---\nid: BFT-001\ntitle: Test\nstatus: Draft\n---\n# Test\n",
        );
        generate_impl(&repo, input, &mut out).unwrap();
        assert!(out.contains(MARKER_START));
        assert!(out.contains(MARKER_END));
        assert!(!out.contains("old"));
        assert!(out.starts_with("Hello\n"));
        assert!(out.ends_with("\nWorld"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn no_markers_appends() {
        let input = "# Project\nSome content";
        let mut out = String::new();
        let tmp = std::env::temp_dir();
        let repo = tmp.join("kq_readme_test2");
        let _ = std::fs::create_dir_all(&repo);
        let _ = std::fs::write(repo.join("README.md"), input);
        generate_impl(&repo, input, &mut out).unwrap();
        assert!(out.starts_with("# Project\nSome content"));
        assert!(out.contains(MARKER_START));
        assert!(out.contains(MARKER_END));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn kanban_contains_summary_line() {
        let kanban = build_kanban().unwrap();
        assert!(kanban.contains("## Kanban Board"));
        assert!(kanban.contains("Total:"));
    }

    #[test]
    fn kanban_has_all_columns() {
        let kanban = build_kanban().unwrap();
        assert!(kanban.contains("| Todo "));
        assert!(kanban.contains("| In Progress "));
        assert!(kanban.contains("| Review "));
        assert!(kanban.contains("| Done "));
    }

    #[test]
    fn kanban_table_structure() {
        let kanban = build_kanban().unwrap();
        assert!(kanban.contains("| --- "));
        let lines: Vec<&str> = kanban.lines().collect();
        let header_idx = lines.iter().position(|l| l.contains("| Todo ")).unwrap();
        let sep_idx = lines.iter().position(|l| l.contains("| --- ")).unwrap();
        assert_eq!(sep_idx, header_idx + 1, "separator must follow header");
    }

    #[test]
    fn toc_contains_table_headers() {
        let tmp = std::env::temp_dir();
        let repo = tmp.join("kq_readme_toc");
        let _ = std::fs::create_dir_all(&repo.join("docs/01-business-foundation"));
        let _ = std::fs::write(
            repo.join("docs/01-business-foundation/bft-001-test.md"),
            "---\nid: BFT-001\ntitle: Test\nstatus: Draft\n---\n# Test\n",
        );
        let toc = super::build_doc_toc(&repo).unwrap();
        assert!(toc.contains("| ID | Title | Status |"), "TOC should have table header");
        assert!(toc.contains("[BFT-001]"), "TOC should list BFT-001 as link");
        assert!(toc.contains("|----|-------|--------|"), "TOC should have separator");
        let _ = std::fs::remove_dir_all(&repo);
    }
}

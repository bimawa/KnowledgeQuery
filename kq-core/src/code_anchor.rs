use std::path::Path;

use anyhow::Result;
use regex::Regex;
use sha2::{Digest, Sha256};

/// A code anchor or @see reference found in source files.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeAnchor {
    pub anchor: String,
    pub repo_path: String,
    pub file_path: String,
    pub line_number: u32,
    pub anchor_type: AnchorType,
    pub target_doc: Option<String>,
    pub file_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnchorType {
    DocAnchor,
    SeeRef,
}

impl AnchorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnchorType::DocAnchor => "doc_anchor",
            AnchorType::SeeRef => "see_ref",
        }
    }
}

/// Result summary for scanning a project.
#[derive(Debug, Default)]
pub struct ScanSummary {
    pub total_files: u32,
    pub total_anchors: u32,
    pub total_see_refs: u32,
}

/// Hash a string for change detection.
fn sha2_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Parse a single source file for `@doc-anchor` and `@see` annotations.
pub fn scan_file(path: &Path, repo_path: &str) -> Result<Vec<CodeAnchor>> {
    let content = std::fs::read_to_string(path)?;
    let hash = sha2_hex(&content);
    let repo_path = repo_path.to_string();
    let file_path = path.to_string_lossy().to_string();

    let doc_anchor_re = Regex::new(r"@doc-anchor\s+(\S+)")?;
    let see_ref_re = Regex::new(r"@see\s+docs://(\S+)")?;

    let mut anchors = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        let line_num = (line_num + 1) as u32;

        if let Some(caps) = doc_anchor_re.captures(line) {
            let anchor = caps[1].to_string();
            // Verify it's inside a comment
            if is_comment_line(line) {
                anchors.push(CodeAnchor {
                    anchor,
                    repo_path: repo_path.clone(),
                    file_path: file_path.clone(),
                    line_number: line_num,
                    anchor_type: AnchorType::DocAnchor,
                    target_doc: None,
                    file_hash: hash.clone(),
                });
            }
        }

        if let Some(caps) = see_ref_re.captures(line) {
            let target = caps[1].to_string();
            if is_comment_line(line) {
                anchors.push(CodeAnchor {
                    anchor: format!("see:{}", target),
                    repo_path: repo_path.clone(),
                    file_path: file_path.clone(),
                    line_number: line_num,
                    anchor_type: AnchorType::SeeRef,
                    target_doc: Some(target),
                    file_hash: hash.clone(),
                });
            }
        }
    }

    Ok(anchors)
}

/// Check if a line is inside a comment (starts with comment marker).
fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
}

/// Check if a path should be ignored.
fn is_ignored(path: &Path, ignore_patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    // Always ignore common non-source dirs
    const ALWAYS_IGNORE: &[&str] = &[
        ".git", "node_modules", "target", ".obsidian",
        "__pycache__", ".build", "Pods", ".venv",
    ];
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            if ALWAYS_IGNORE.contains(&name) {
                return true;
            }
        }
    }
    // User-configured ignore patterns
    for pattern in ignore_patterns {
        if path_str.contains(pattern) {
            return true;
        }
    }
    false
}

/// Check if a file matches any scan pattern (by extension).
fn matches_scan_pattern(path: &Path, patterns: &[String]) -> bool {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    for pattern in patterns {
        if pattern.ends_with(&ext) {
            return true;
        }
    }
    false
}

/// Scan a project directory for code anchors.
pub fn scan_project(
    project_path: &Path,
    scan_patterns: &[String],
    ignore_patterns: &[String],
) -> Result<ScanSummary> {
    let mut summary = ScanSummary::default();
    let repo_path = project_path.to_string_lossy().to_string();
    let db = crate::db::get_db()?;

    // Clear old anchors for this project
    let _ = crate::db::clear_project_anchors(&db, &repo_path);

    for entry in walkdir::WalkDir::new(project_path)
        .into_iter()
        .filter_entry(|e| !is_ignored(e.path(), ignore_patterns))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !matches_scan_pattern(path, scan_patterns) {
            continue;
        }

        let anchors = scan_file(path, &repo_path)?;
        if !anchors.is_empty() {
            summary.total_files += 1;
            for a in &anchors {
                match a.anchor_type {
                    AnchorType::DocAnchor => summary.total_anchors += 1,
                    AnchorType::SeeRef => summary.total_see_refs += 1,
                }
                crate::db::upsert_code_anchor(
                    &db, &a.anchor, &a.repo_path, &a.file_path,
                    a.line_number, a.anchor_type.as_str(),
                    a.target_doc.as_deref(), &a.file_hash,
                )?;
            }
        }
    }

    Ok(summary)
}

    #[test]
    fn test_scan_file_doc_anchor_rust_style() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "// @doc-anchor SecureTokenStorage\nfn main() {}").unwrap();

        let anchors = scan_file(&file_path, "/test/project").unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].anchor, "SecureTokenStorage");
        assert_eq!(anchors[0].anchor_type, AnchorType::DocAnchor);
        assert_eq!(anchors[0].line_number, 1);
    }

    #[test]
    fn test_scan_file_doc_anchor_python_style() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.py");
        std::fs::write(&file_path, "# @doc-anchor JwtValidator\ndef validate(): pass").unwrap();

        let anchors = scan_file(&file_path, "/test/project").unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].anchor, "JwtValidator");
    }

    #[test]
    fn test_scan_file_see_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.go");
        std::fs::write(
            &file_path,
            "// @see docs://architecture/ADR-0002.md\npackage main",
        )
        .unwrap();

        let anchors = scan_file(&file_path, "/test/project").unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].anchor_type, AnchorType::SeeRef);
        assert_eq!(anchors[0].target_doc.as_deref(), Some("architecture/ADR-0002.md"));
    }

    #[test]
    fn test_scan_file_no_false_positive() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("code.rs");
        // @doc-anchor inside a string, not a comment — should be ignored
        std::fs::write(&file_path, r#"let s = "// @doc-anchor NotARealAnchor";"#).unwrap();

        let anchors = scan_file(&file_path, "/test/project").unwrap();
        assert!(anchors.is_empty(), "anchors inside strings should be ignored");
    }

    #[test]
    fn test_scan_project_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let project_dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        crate::db::init_db(&db_path).unwrap();
        let summary = scan_project(project_dir.path(), &["**/*.rs".to_string()], &[]).unwrap();
        assert_eq!(summary.total_files, 0);
        assert_eq!(summary.total_anchors, 0);
    }

    #[test]
    fn test_is_ignored_git_dir() {
        assert!(is_ignored(Path::new("/project/.git/config"), &[]));
        assert!(is_ignored(Path::new("/project/node_modules/foo.js"), &[]));
        assert!(!is_ignored(Path::new("/project/src/main.rs"), &[]));
    }

    #[test]
    fn test_matches_scan_pattern() {
        assert!(matches_scan_pattern(Path::new("main.rs"), &["**/*.rs".to_string()]));
        assert!(matches_scan_pattern(Path::new("app.swift"), &["**/*.swift".to_string()]));
        assert!(!matches_scan_pattern(Path::new("readme.md"), &["**/*.rs".to_string()]));
    }

    #[test]
    fn test_sha2_hex_deterministic() {
        let h1 = sha2_hex("hello");
        let h2 = sha2_hex("hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

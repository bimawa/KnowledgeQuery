use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use std::path::PathBuf;

const TEMPLATES_SUBDIR: &str = ".kq/templates";


const GROK_CATEGORIES: &[(&str, &str)] = &[
    ("01-business-foundation", "Business Foundation"),
    ("02-product-ux", "Product & UX"),
    ("03-architecture", "Architecture"),
    ("04-technical-design", "Technical Design"),
    ("05-ideas", "Ideas"),
    ("06-glossary", "Glossary"),
    ("07-standards", "Standards"),
    ("08-reference", "Reference"),
];

const DOC_TYPES: &[(&str, &str)] = &[
    ("bft", "Business Foundation Template"),
    ("brd", "Business Requirements Document"),
    ("frd", "Functional Requirements Document"),
    ("nfr", "Non-Functional Requirements"),
    ("adr", "Architecture Decision Record"),
    ("rfc", "Request for Comments"),
    ("tz", "Technical Design"),
    ("idea", "Idea"),
    ("user_story", "User Story"),
    ("glossary", "Glossary Entry"),
    ("screen", "Screen Design"),
    ("userflow", "User Flow"),
];

const DOC_CATEGORY_MAP: &[(&str, &str)] = &[
    ("bft", "01-business-foundation"),
    ("brd", "01-business-foundation"),
    ("frd", "02-product-ux"),
    ("nfr", "01-business-foundation"),
    ("adr", "03-architecture"),
    ("rfc", "03-architecture"),
    ("tz", "04-technical-design"),
    ("idea", "05-ideas"),
    ("user_story", "02-product-ux"),
    ("glossary", "06-glossary"),
    ("screen", "04-technical-design"),
    ("userflow", "04-technical-design"),
];

/// Get the human-readable label for a category slug.
pub fn category_label(slug: &str) -> &str {
    GROK_CATEGORIES
        .iter()
        .find(|(s, _)| *s == slug)
        .map(|(_, n)| *n)
        .unwrap_or(slug)
}

/// Document type determined from file path prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocType {
    Bft,
    Brd,
    Frd,
    Nfr,
    Adr,
    Rfc,
    Tz,
    Idea,
    UserStory,
    Glossary,
    Screen,
    Userflow,
    Typespec,
}

impl std::fmt::Display for DocType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocType::Bft => write!(f, "bft"),
            DocType::Brd => write!(f, "brd"),
            DocType::Frd => write!(f, "frd"),
            DocType::Nfr => write!(f, "nfr"),
            DocType::Adr => write!(f, "adr"),
            DocType::Rfc => write!(f, "rfc"),
            DocType::Tz => write!(f, "tz"),
            DocType::Idea => write!(f, "idea"),
            DocType::UserStory => write!(f, "user_story"),
            DocType::Glossary => write!(f, "glossary"),
            DocType::Screen => write!(f, "screen"),
            DocType::Userflow => write!(f, "userflow"),
            DocType::Typespec => write!(f, "typespec"),
        }
    }
}

const DOC_TYPE_PREFIXES: &[(&str, DocType)] = &[
    ("bft-", DocType::Bft),
    ("brd-", DocType::Brd),
    ("frd-", DocType::Frd),
    ("nfr-", DocType::Nfr),
    ("adr-", DocType::Adr),
    ("rfc-", DocType::Rfc),
    ("tz-", DocType::Tz),
    ("idea-", DocType::Idea),
    ("user_story-", DocType::UserStory),
    ("glossary-", DocType::Glossary),
    ("screen-", DocType::Screen),
    ("userflow-", DocType::Userflow),
];

/// Detect document type from filename prefix.
/// e.g. "bft-001-title.md" -> Some(DocType::Bft)
pub fn detect_doc_type(filename: &str) -> Option<DocType> {
    let basename = Path::new(filename).file_stem()?.to_str()?;
    for &(prefix, doc_type) in DOC_TYPE_PREFIXES {
        if basename.starts_with(prefix) {
            return Some(doc_type);
        }
    }
    None
}


/// Parsed traceable document node from a .md file.
#[derive(Debug, Clone)]
pub struct DocNode {
    pub id: String,
    pub doc_type: DocType,
    pub title: String,
    pub file_path: String,
    pub revision: u32,
    pub status: String,
    pub category: Option<String>,
    pub needs: Vec<String>,
    pub covers: Vec<String>,
    pub bft_refs: Vec<String>,
    pub rfc_refs: Vec<String>,
    pub code_anchors: Vec<String>,
    pub inline_refs: Vec<String>,
}

/// Parse YAML front matter from a markdown file content.
/// Looks for content between --- and --- delimiters.
fn parse_front_matter(content: &str) -> serde_yaml::Value {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return serde_yaml::Value::Null;
    }
    let end_marker = "\n---\n";
    if let Some(end) = content[4..].find(end_marker) {
        let yaml_str = &content[4..4 + end];
        serde_yaml::from_str(yaml_str).unwrap_or(serde_yaml::Value::Null)
    } else {
        serde_yaml::Value::Null
    }
}

/// Parse inline @doc references from markdown content (after front matter).
fn parse_inline_doc_refs(content: &str) -> Vec<String> {
    let body = if content.starts_with("---") {
        if let Some(end) = content[4..].find("\n---\n") {
            &content[4 + end + 5..]
        } else {
            content
        }
    } else {
        content
    };
    let mut refs = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("@doc ") {
            let after = &trimmed[pos + 5..];
            let id: String = after.chars().take_while(|c| *c != ' ' && *c != '\t').collect();
            if !id.is_empty() {
                refs.push(id);
            }
        }
    }
    refs
}

/// Parse a traceable document node from a .md file.
/// Extracts Front Matter + inline @doc references.
pub fn parse_doc_node(path: &Path) -> Result<DocNode> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let file_path = path.to_string_lossy().to_string();
    let filename = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let doc_type = detect_doc_type(&filename)
        .ok_or_else(|| anyhow::anyhow!("Cannot detect doc type from filename: {}", filename))?;
    let fm = parse_front_matter(&content);
    let title = fm.get("title")
        .and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
    let status = fm.get("status")
        .and_then(|v| v.as_str()).unwrap_or("Draft").to_string();
    let revision: u32 = fm.get("revision")
        .and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(1);
    let category = fm.get("category").and_then(|v| v.as_str()).map(String::from);
    let needs = fm.get("needs").and_then(|v| v.as_sequence())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let covers = fm.get("covers").and_then(|v| v.as_sequence())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let bft_refs = fm.get("bft_refs").and_then(|v| v.as_sequence())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let rfc_refs = fm.get("rfc_refs").and_then(|v| v.as_sequence())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let code_anchors = fm.get("code_anchors").and_then(|v| v.as_sequence())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let inline_refs = parse_inline_doc_refs(&content);
    let id = fm.get("id").and_then(|v| v.as_str()).map(String::from)
        .unwrap_or_else(|| filename.strip_suffix(".md").unwrap_or(&filename).to_string());
    Ok(DocNode { id, doc_type, title, file_path, revision, status, category,
        needs, covers, bft_refs, rfc_refs, code_anchors, inline_refs })
}

/// List all doc files in the knowledge repo with their auto-detected type.
pub fn list_doc_nodes(path: &Path) -> Result<Vec<DocNode>> {
    let doc_files = list_docs(path)?;
    let mut nodes = Vec::new();
    for (_cat, filepath) in &doc_files {
        if let Ok(node) = parse_doc_node(Path::new(filepath)) {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

pub fn init_with_docs(path: &Path) -> Result<()> {
    let docs_dir = path.join("docs");
    fs::create_dir_all(&docs_dir)
        .with_context(|| format!("Failed to create docs/ at {}", docs_dir.display()))?;

    for (slug, _name) in GROK_CATEGORIES {
        let category_dir = docs_dir.join(slug);
        fs::create_dir_all(&category_dir)
            .with_context(|| format!("Failed to create docs/{}/", slug))?;
    }

    Ok(())
}

pub fn generate_doc(path: &Path, doc_type: &str, title: &str) -> Result<String> {
    // Support any type: known DOC_TYPES OR custom template file
    let type_label = if DOC_TYPES.iter().any(|(t, _)| *t == doc_type) {
        DOC_TYPES.iter().find(|(t, _)| *t == doc_type).map(|(_, l)| *l).unwrap_or("Document")
    } else {
        let template_file = path.join(TEMPLATES_SUBDIR).join(format!("{}.md", doc_type));
        if !template_file.exists() {
            anyhow::bail!(
                "Unknown doc type '{}'. Use one of: {} or create .kq/templates/{}.md",
                doc_type,
                DOC_TYPES.iter().map(|(t, _)| *t).collect::<Vec<_>>().join(", "),
                doc_type
            );
        }
        "Custom Document"
    };

    let category = DOC_CATEGORY_MAP
        .iter()
        .find(|(t, _)| *t == doc_type)
        .map(|(_, c)| *c)
        .unwrap_or("05-ideas");

    let category_label = GROK_CATEGORIES
        .iter()
        .find(|(s, _)| *s == category)
        .map(|(_, n)| *n)
        .unwrap_or("Ideas");

    let slug = slugify(title);

    let docs_dir = path.join("docs");
    let category_dir = docs_dir.join(category);
    fs::create_dir_all(&category_dir)
        .with_context(|| format!("Failed to create docs/{}/", category))?;

    let next_num = next_doc_number(&category_dir, doc_type)?;

    let filename = format!("{}-{:03}-{}.md", doc_type, next_num, slug);
    let file_path = category_dir.join(&filename);

    let author = get_author();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let content = render_template(path, doc_type, title, &author, &date, &category_label)?;

    fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write doc at {}", file_path.display()))?;

    Ok(file_path.display().to_string())
}

pub fn list_docs(path: &Path) -> Result<Vec<(String, String)>> {
    let docs_dir = path.join("docs");
    if !docs_dir.exists() {
        return Ok(vec![]);
    }

    let mut results = Vec::new();

    let entries = fs::read_dir(&docs_dir)
        .with_context(|| format!("Failed to read docs/ at {}", docs_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let category_dir = entry.path();
        let category_name = entry.file_name().to_string_lossy().to_string();

        let doc_files = fs::read_dir(&category_dir)
            .with_context(|| format!("Failed to read {}", category_dir.display()))?;

        for doc_entry in doc_files {
            let doc_entry = doc_entry?;
            if doc_entry.file_type()?.is_dir() {
                continue;
            }
            let fname = doc_entry.file_name().to_string_lossy().to_string();
            if fname.ends_with(".md") {
                let full_path = doc_entry.path().display().to_string();
                results.push((category_name.clone(), full_path));
            }
        }
    }

    results.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(results)
}

/// Initialize template files in `.kq/templates/` for all document types.
/// Creates the directory and writes default template `.md` files.
pub fn init_templates(repo_path: &Path) -> Result<()> {
    let templates_dir = repo_path.join(TEMPLATES_SUBDIR);
    fs::create_dir_all(&templates_dir)
        .with_context(|| format!("Failed to create templates dir at {}", templates_dir.display()))?;

    for (doc_type, _label) in DOC_TYPES {
        let (_type_label, body) = template_content(doc_type);
        let file_path = templates_dir.join(format!("{}.md", doc_type));
        if !file_path.exists() {
            fs::write(&file_path, body)
                .with_context(|| format!("Failed to write template {}", file_path.display()))?;
        }
    }

    Ok(())
}

/// Load template body from `.kq/templates/<type>.md` file, falling back to hardcoded template.
fn load_template(repo_path: &Path, doc_type: &str) -> Result<(&'static str, String)> {
    let template_file = repo_path.join(TEMPLATES_SUBDIR).join(format!("{}.md", doc_type));

    if template_file.exists() {
        let body = fs::read_to_string(&template_file)
            .with_context(|| format!("Failed to read template {}", template_file.display()))?;
        let (type_label, _) = template_content(doc_type);
        return Ok((type_label, body));
    }

    let (type_label, body) = template_content(doc_type);
    Ok((type_label, body.to_string()))
}

pub fn templates_list(repo_path: Option<&Path>) -> Vec<String> {
    let mut types: Vec<String> = DOC_TYPES.iter().map(|(t, _)| t.to_string()).collect();

    // Scan custom templates from filesystem
    if let Some(path) = repo_path {
        let templates_dir = path.join(TEMPLATES_SUBDIR);
        if templates_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&templates_dir) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if let Some(base) = fname.strip_suffix(".md") {
                        if !types.contains(&base.to_string()) {
                            types.push(base.to_string());
                        }
                    }
                }
            }
        }
    }

    types.sort();
    types
}

fn slugify(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn next_doc_number(category_dir: &Path, doc_type: &str) -> Result<u32> {
    let prefix = format!("{}-", doc_type);
    let mut max_num: u32 = 0;

    if category_dir.exists() {
        let entries = fs::read_dir(category_dir)
            .with_context(|| format!("Failed to read {}", category_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().to_string();

            if let Some(rest) = fname.strip_prefix(&prefix) {
                if let Some(num_part) = rest.split('-').next() {
                    if let Ok(num) = num_part.parse::<u32>() {
                        if num > max_num {
                            max_num = num;
                        }
                    }
                }
            }
        }
    }

    Ok(max_num + 1)
}

fn get_author() -> String {
    let output = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output();

    if let Ok(out) = output {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }

    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
fn render_template(repo_path: &Path, doc_type: &str, title: &str, author: &str, date: &str, category: &str) -> Result<String> {
    let (type_label, body) = load_template(repo_path, doc_type)?;

    Ok(format!(
        "---\ntitle: \"{title}\"\ntype: {type_label}\ncategory: \"{category}\"\nstatus: Draft\ndate: {date}\nauthor: \"{author}\"\n---\n\n# {title}\n\n{body}"
    ))
}

fn template_content(doc_type: &str) -> (&'static str, &'static str) {
    match doc_type {
        "bft" => (
            "Business Foundation",
            "## Overview\n\nDescribe the foundational business context.\n\n## Objectives\n\n- \n\n## Stakeholders\n\n| Name | Role |\n|------|------|\n|      |      |\n\n## Scope\n\n### In Scope\n\n- \n\n### Out of Scope\n\n- \n\n"
        ),
        "brd" => (
            "Business Requirements Document",
            "## Executive Summary\n\nBrief overview of the business need.\n\n## Business Requirements\n\n| ID | Requirement | Priority | Status |\n|----|------------|----------|--------|\n| BR-001 | | High | Draft |\n\n## Assumptions\n\n- \n\n## Constraints\n\n- \n\n## Acceptance Criteria\n\n- \n\n"
        ),
        "frd" => (
            "Functional Requirements Document",
            "## Functional Requirements\n\n| ID | Requirement | Priority | Status |\n|----|------------|----------|--------|\n| FR-001 | | High | Draft |\n\n## User Interface\n\n- \n\n## Business Rules\n\n- \n\n## Data Requirements\n\n- \n\n## Error Handling\n\n- \n\n"
        ),
        "nfr" => (
            "Non-Functional Requirements",
            "## Performance\n\n- Response time: \n- Throughput: \n\n## Security\n\n- Authentication: \n- Authorization: \n- Data encryption: \n\n## Availability\n\n- Uptime target: \n- Recovery time: \n\n## Scalability\n\n- \n\n## Compatibility\n\n- \n\n"
        ),
        "adr" => (
            "Architecture Decision Record",
            "## Status\n\nProposed\n\n## Context\n\nWhat is the issue?\n\n## Decision\n\nWhat was decided?\n\n## Consequences\n\n### Positive\n\n- \n\n### Negative\n\n- \n\n### Neutral\n\n- \n\n## Alternatives Considered\n\n- \n\n"
        ),
        "rfc" => (
            "Request for Comments",
            "## Summary\n\nBrief one-liner.\n\n## Motivation\n\nWhy this change?\n\n## Detailed Design\n\n### Overview\n\n- \n\n### Implementation\n\n- \n\n## Alternatives\n\n- \n\n## Unresolved Questions\n\n- \n\n## References\n\n- \n\n"
        ),
        "tz" => (
            "Technical Design",
            "## Overview\n\nHigh-level design summary.\n\n## Architecture\n\n- \n\n## API Design\n\n### Endpoints\n\n| Method | Path | Description |\n|--------|------|-------------|\n| GET | / | |\n\n### Data Model\n\n- \n\n## Implementation Plan\n\n1. \n\n## Testing Strategy\n\n- \n\n"
        ),
        "idea" => (
            "Idea",
            "## Problem Statement\n\nWhat problem does this solve?\n\n## Proposed Solution\n\n- \n\n## Impact\n\n- \n\n## Effort Estimate\n\n- \n\n## Next Steps\n\n- \n\n"
        ),
        "user_story" => (
            "User Story",
            "## Story\n\nAs a [role], I want [feature] so that [benefit].\n\n## Acceptance Criteria\n\n- [ ] \n\n## UI/UX Notes\n\n- \n\n## Technical Notes\n\n- \n\n## Dependencies\n\n- \n\n"
        ),
        "glossary" => (
            "Glossary Entry",
            "## Definition\n\n- \n\n## Context\n\nHow this term is used in the project.\n\n## Related Terms\n\n- \n\n## Examples\n\n- \n\n"
        ),
        "screen" => (
            "Screen Design",
            "## Overview\n\nWhat this screen shows.\n\n## Layout\n\n- \n\n## Components\n\n| Component | Type | Description |\n|-----------|------|-------------|\n| | | |\n\n## States\n\n| State | Description |\n|-------|-------------|\n| Loading | |\n| Empty | |\n| Error | |\n\n## Interactions\n\n- \n\n"
        ),
        "userflow" => (
            "User Flow",
            "## Flow Name\n\n- \n\n## Steps\n\n1. \n\n## Decision Points\n\n| Point | Yes | No |\n|-------|-----|----|\n| | | |\n\n## Error Paths\n\n- \n\n## Success Criteria\n\n- \n\n"
        ),
        _ => (
            "Document",
            "## Content\n\n- \n\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_temp_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[test]
    fn test_init_with_docs_creates_categories() {
        let dir = setup_temp_dir();
        let path = dir.path();

        init_with_docs(path).expect("init_with_docs failed");

        for (slug, _name) in GROK_CATEGORIES {
            let cat_dir = path.join("docs").join(slug);
            assert!(cat_dir.exists(), "Expected category dir {} to exist", slug);
        }
    }

    #[test]
    fn test_templates_list() {
        let templates = templates_list(None);
        assert_eq!(templates.len(), 12);
        assert!(templates.contains(&"bft".to_string()));
        assert!(templates.contains(&"brd".to_string()));
        assert!(templates.contains(&"frd".to_string()));
        assert!(templates.contains(&"nfr".to_string()));
        assert!(templates.contains(&"adr".to_string()));
        assert!(templates.contains(&"rfc".to_string()));
        assert!(templates.contains(&"tz".to_string()));
        assert!(templates.contains(&"idea".to_string()));
        assert!(templates.contains(&"user_story".to_string()));
        assert!(templates.contains(&"glossary".to_string()));
        assert!(templates.contains(&"screen".to_string()));
        assert!(templates.contains(&"userflow".to_string()));
    }

    #[test]
    fn test_generate_doc_creates_file() {
        let dir = setup_temp_dir();
        let path = dir.path();

        init_with_docs(path).expect("init_with_docs failed");

        let result = generate_doc(path, "bft", "Test Foundation").expect("generate_doc failed");
        assert!(result.contains("bft-001-test-foundation.md"));

        let file_path = PathBuf::from(&result);
        assert!(file_path.exists(), "Generated doc file should exist");

        let content = fs::read_to_string(&file_path).expect("Failed to read generated doc");
        assert!(content.contains("title: \"Test Foundation\""));
        assert!(content.contains("type: Business Foundation"));
        assert!(content.contains("status: Draft"));
        assert!(content.contains("author:"));
    }

    #[test]
    fn test_generate_doc_auto_numbers() {
        let dir = setup_temp_dir();
        let path = dir.path();

        init_with_docs(path).expect("init_with_docs failed");

        let first = generate_doc(path, "rfc", "First RFC").expect("first generate_doc failed");
        assert!(first.contains("rfc-001-first-rfc.md"));

        let second = generate_doc(path, "rfc", "Second RFC").expect("second generate_doc failed");
        assert!(second.contains("rfc-002-second-rfc.md"));
    }

    #[test]
    fn test_generate_doc_unknown_type() {
        let dir = setup_temp_dir();
        let path = dir.path();

        let result = generate_doc(path, "invalid_type", "Test");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_docs_empty() {
        let dir = setup_temp_dir();
        let path = dir.path();

        let docs = list_docs(path).expect("list_docs failed");
        assert!(docs.is_empty());
    }

    #[test]
    fn test_list_docs_returns_files() {
        let dir = setup_temp_dir();
        let path = dir.path();

        init_with_docs(path).expect("init_with_docs failed");
        generate_doc(path, "bft", "Foundation Doc").expect("generate_doc failed");
        generate_doc(path, "adr", "Decision Record").expect("generate_doc failed");

        let docs = list_docs(path).expect("list_docs failed");
        assert_eq!(docs.len(), 2);

        let categories: Vec<&str> = docs.iter().map(|(c, _)| c.as_str()).collect();
        assert!(categories.contains(&"01-business-foundation"));
        assert!(categories.contains(&"03-architecture"));
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("My Cool Feature"), "my-cool-feature");
        assert_eq!(slugify("special!@#chars"), "special-chars");
        assert_eq!(slugify("Multiple   Spaces"), "multiple-spaces");
    }

    #[test]
    fn test_render_template_has_frontmatter() {
        let dir = setup_temp_dir();
        let repo_path = dir.path();
        init_templates(repo_path).expect("init_templates failed");

        let content = render_template(repo_path, "adr", "Test ADR", "Author", "2024-01-01", "Architecture").unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("title: \"Test ADR\""));
        assert!(content.contains("type: Architecture Decision Record"));
        assert!(content.contains("status: Draft"));
    }
}

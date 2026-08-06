# Design Document: Trace Graph Database & Deep Coverage

> **Spec:** `trace-graph-db`
> **Language:** ru
> **Status:** Draft

## Overview

Замена одноразового парсинга `@doc`-ссылок в `check.rs` на SQLite-граф связей.
Граф — кеш, источник истины — .md файлы. Deep coverage через транзитивное замыкание.

## Architecture

### Current Architecture (before)

```
check.rs:
  list_docs() → scan .md for @doc → build HashMap graph → print report
  list_types() → scan .tsp for @doc → merge → orphans/broken/dangling
```

Всё в памяти, не сохраняется, O(n) парсинг при каждом запуске.
Нет: типов связей, ревизий, deep coverage.

### New Architecture (after)

```
┌──────────────────────────────────────────────────┐
│                    check.rs                       │
│  parse_front_matter() → upsert trace_nodes        │
│  parse_inline_doc_refs() → upsert trace_links     │
│  compute_deep_coverage() → graph traversal        │
│  print_report() → from SQL, not HashMap           │
└──────────────┬───────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────┐
│              db.rs (Trace Schema)                  │
│  trace_nodes: node_id, type, title, revision,     │
│               status, file_path, created_at       │
│  trace_links: source_id, target_id, link_type,    │
│               status, created_at                  │
│  schema_version: version, last_indexed_commit     │
└──────────────────────────────────────────────────┘
```

### Module Changes

| Module | Change | Impact |
|--------|--------|--------|
| `db.rs` | + `create_trace_schema()`, + `get_trace_db()` | New schema version |
| `check.rs` | Rewrite `traceability()` to use SQL | New algorithm |
| `docs.rs` | + `parse_front_matter(path)`, + `get_doc_type_from_path()` | New functions |
| `typespec.rs` | No change needed | — |
| `cli/main.rs` | + `--deep`, `--chain`, `--json` flags for `kq check` | New args |

## SQLite Schema

```sql
-- Version tracking for trace graph
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    last_indexed_commit TEXT,
    indexed_at TEXT NOT NULL
);

-- Nodes: each traceable entity (doc, typespec model)
CREATE TABLE IF NOT EXISTS trace_nodes (
    node_id TEXT PRIMARY KEY,          -- e.g. "BFT-001", "ADR-0002", "TypeSpec/User"
    node_type TEXT NOT NULL,           -- bft | brd | frd | nfr | adr | rfc | tz | idea |
                                       -- user_story | glossary | screen | userflow | typespec
    title TEXT NOT NULL,
    file_path TEXT NOT NULL,           -- relative path from knowledge root
    revision INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',  -- active | removed
    category TEXT,                     -- "01-business-foundation", etc.
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Links: directional connections between nodes
CREATE TABLE IF NOT EXISTS trace_links (
    link_id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES trace_nodes(node_id),
    target_id TEXT NOT NULL REFERENCES trace_nodes(node_id),
    link_type TEXT NOT NULL,           -- covers | needs | references | implements
    status TEXT NOT NULL DEFAULT 'valid',  -- valid | stale | broken
    detected_at TEXT NOT NULL,
    UNIQUE(source_id, target_id, link_type)
);

-- Indexes for fast traversal
CREATE INDEX IF NOT EXISTS idx_trace_links_source ON trace_links(source_id);
CREATE INDEX IF NOT EXISTS idx_trace_links_target ON trace_links(target_id);
CREATE INDEX IF NOT EXISTS idx_trace_nodes_type ON trace_links(link_type);
CREATE INDEX IF NOT EXISTS idx_trace_nodes_status ON trace_nodes(status);
```

### Link Types

| `link_type` | Direction | Meaning | Example |
|---|---|---|---|
| `needs` | BFT → ADR | BFT-001 needs ADR coverage | BFT-001 `needs: ["ADR", "TZ"]` |
| `covers` | TZ → BFT | TZ-001 covers BFT-001 | TZ-001 `covers: ["BFT-001"]` |
| `references` | any → any | Inline @doc reference | In body: `@doc BFT-001` |
| `implements` | TypeSpec → TZ | Model реализует ТЗ | Model `// @doc TZ-001` |

## Front Matter Parser

Расширяем `docs.rs` новым модулем `trace_parser`:

```rust
pub struct DocNode {
    pub id: String,                    // "BFT-001"
    pub doc_type: DocType,             // enum Bft, Adr, Tz, ...
    pub title: String,
    pub file_path: String,
    pub revision: u32,                 // из поля revision в Front Matter, def: 1
    pub status: String,                // из поля status: Draft | Proposed | Approved
    pub category: Option<String>,
    pub needs: Vec<String>,            // из поля needs: ["ADR", "TZ"]
    pub covers: Vec<String>,           // из поля covers: ["BFT-001"]
    pub bft_refs: Vec<String>,         // из поля bft_refs
    pub rfc_refs: Vec<String>,         // из поля rfc_refs
    pub code_anchors: Vec<String>,     // из поля code_anchors (готовимся к Wave 2)
    pub inline_refs: Vec<String>,      // из inline @doc XXX-NNN в теле
}

pub fn parse_doc_node(path: &Path) -> Result<DocNode> {
    // 1. Read file
    // 2. Detect doc_type from filename prefix (BFT-* → Bft, ADR-* → Adr)
    // 3. Front Matter YAML: extract all known fields
    // 4. Body: regex @doc (\w+-\d+) for inline refs
    // 5. Merge body refs with frontmatter refs
    // 6. Return DocNode
}
```

### DocType detection (from filepath)

```rust
pub fn detect_doc_type(filename: &str) -> Option<DocType> {
    // "bft-001-title.md" → Some(Bft)
    // "docs/03-architecture/adr-002-*.md" → Some(Adr)
    let prefixes = [
        ("bft-", Bft), ("brd-", Brd), ("frd-", Frd), ("nfr-", Nfr),
        ("adr-", Adr), ("rfc-", Rfc), ("tz-", Tz), ("idea-", Idea),
        ("user_story-", UserStory), ("glossary-", Glossary),
        ("screen-", Screen), ("userflow-", Userflow),
    ];
    let basename = Path::new(filename).file_stem()?.to_str()?;
    for (prefix, doc_type) in prefixes {
        if basename.starts_with(prefix) { return Some(doc_type); }
    }
    None
}
```

## Deep Coverage Algorithm

### Chain Definition

```rust
const DEFAULT_CHAIN: &[(DocType, DocType)] = &[
    (Bft, Brd),
    (Brd, Frd),
    (Brd, Nfr),
    (Frd, Adr),
    (Nfr, Adr),
    (Adr, Tz),
    (Tz, Typespec),
];

// terminating types — don't need further coverage
const TERMINATING_TYPES: &[DocType] = &[Typespec, Idea, Glossary];
```

### Algorithm

```
function compute_deep_coverage(trace_db, chain_definition):
    for each source_type in chain:
        for each node of source_type:
            // BFS/DFS: существует ли путь от node до terminating_type?
            path = find_path(graph, node.node_id, terminating_types)
            
            if path is empty:
                mark chain as BROKEN at source_type → next_type
            else:
                for each link in path:
                    if link.status == 'stale':
                        mark chain as STALE at that link
                
            result = {
                node_id: node.node_id,
                chain_status: OK | BROKEN_at_X | STALE_at_X,
                coverage_pct: path.len() / chain.len(),
                missing: types_not_covered,
            }
    return results
```

### SQL Traversal (instead of HashMap)

```sql
-- Find all outgoing links from a node
SELECT target_id, link_type, status FROM trace_links
WHERE source_id = ? AND status = 'valid';

-- Find all incoming links to a node
SELECT source_id, link_type, status FROM trace_links
WHERE target_id = ? AND status = 'valid';

-- Orphan detection: nodes with zero incoming links
SELECT n.node_id, n.node_type, n.title
FROM trace_nodes n
LEFT JOIN trace_links l ON l.target_id = n.node_id
WHERE n.status = 'active' AND l.link_id IS NULL;

-- Stale: links where source revision doesn't match
SELECT l.* FROM trace_links l
JOIN trace_nodes n ON n.node_id = l.source_id
WHERE l.status = 'valid'
  AND l.updated_at < n.updated_at;
```

## Incremental Re-index

```rust
pub fn rebuild_trace_graph(path: &Path) -> Result<()> {
    let db = get_db()?;
    let last_commit = get_last_indexed_commit(&db)?;
    let current_commit = get_head_commit(path)?;
    
    if last_commit == current_commit {
        return Ok(()); // no changes
    }
    
    // Get changed files
    let changed = git_diff_files(path, &last_commit, &current_commit)?;
    
    // Process changes
    for file in changed {
        match file.status {
            Added | Modified => upsert_node(path, &file.path, &mut db)?,
            Deleted => mark_node_removed(&file.path, &mut db)?,
        }
    }
    
    // Update last indexed commit
    set_last_indexed_commit(&mut db, &current_commit)?;
    
    // Recompute stale status
    mark_stale_links(&mut db)?;
    
    Ok(())
}
```

## CLI Changes

```rust
// kq-cli/src/main.rs
Subcommand::Check {
    #[arg(long, help = "Проверить глубокое покрытие (полные цепочки)")]
    deep: bool,

    #[arg(long, help = "Цепочка типов для deep coverage, напр. \"bft adr tz\"")]
    chain: Option<String>,

    #[arg(long, help = "Вывод в JSON")]
    json: bool,
}
```

## File Changes Summary

| File | Change | Lines |
|------|--------|-------|
| `kq-core/src/db.rs` | + `create_trace_schema()`, + trace table creation | +60 |
| `kq-core/src/check.rs` | Rewrite `traceability()` → SQL-based, + `compute_deep_coverage()`, + `--deep/--chain/--json` | ~200 rewrite |
| `kq-core/src/docs.rs` | + `parse_doc_node()`, + `detect_doc_type()`, + `DocNode` struct, + `DocType` enum | +150 |
| `kq-core/src/lib.rs` | Expose new public functions | +5 |
| `kq-cli/src/main.rs` | + `--deep`, `--chain`, `--json` flags | +15 |
| `Cargo.toml` (kq-core) | + `regex` dep | +1 |
| `.kiro/specs/trace-graph-db/spec.json` | phase → "design" | — |

## Testing Strategy

| Test | Type | What |
|------|------|------|
| `test_trace_schema_creation` | unit | Tables created correctly |
| `test_parse_front_matter_needs` | unit | Parses `needs: ["ADR"]` from YAML |
| `test_parse_front_matter_covers` | unit | Parses `covers: ["BFT-001"]` from YAML |
| `test_detect_doc_type` | unit | BFT-001 → Bft, ADR-002 → Adr |
| `test_inline_doc_refs` | unit | `@doc BFT-001` extracted from body |
| `test_deep_coverage_complete` | integration | Full chain → OK |
| `test_deep_coverage_broken` | integration | Missing link → BROKEN |
| `test_stale_detection` | integration | Revision change → stale |
| `test_incremental_reindex` | integration | Changed files only |
| `test_backward_compat` | integration | Old `kq check` still works |

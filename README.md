# kq — Git-native Knowledge Platform

CLI tool for managing knowledge and documentation in Git + Markdown.
Sync, search (full-text + vector), AI assistance, task tracking,
and **traceability between documentation and code** —
all in one binary, no servers or databases.

## Quick Start

### Build

```bash
# Debug
just build

# Release
just release
```

### Run

```bash
cargo run -- <command> [args]
./target/release/kq <command>
```

### Test

```bash
just test
cargo test --workspace -- --nocapture
```

## Dual Mode

kq runs in two contexts:

| Mode | Description | Commands |
|------|-------------|----------|
| **Dev** (`--dev`) | Inside a developer's project. Reads code, does not write to docs | scan, check, watch |
| **Doc** (`--doc`) | Inside a knowledge repository. Full access | everything + doc new, typespec, readme |

```bash
# Explicit
kq --dev check scan
kq --doc doc new adr "Title"

# Auto-detect: if docs/ + TypeSpec/ exist → doc mode
kq check traceability

# Via env
KQ_MODE=dev kq check scan
```

In dev mode, doc-only commands (`kq doc new`, `kq typespec new`, `kq readme`)
fail with an explanatory error.

## Initialization

```bash
# Create a knowledge repository
kq init
kq init --path ~/my-knowledge --remote https://github.com/user/knowledge.git
```

## Working with Documents

```bash
# List templates
kq doc template --list

# Create a document
kq doc new adr "Database Choice"      # → docs/03-architecture/adr-001-*.md
kq doc new tz "API Gateway Design"    # → docs/04-technical-design/tz-001-*.md
kq doc new bft "Customer Portal"      # → docs/01-business-foundation/bft-001-*.md
kq doc new frd "Sign-up"              # → docs/02-product-ux/frd-001-*.md

# Screen and User Flow
kq screen "Login Screen"
kq userflow "Onboarding Flow"

# List all documents
kq doc list
```

## TypeSpec — Data Specifications

```bash
# Create a model
kq typespec new User
# → TypeSpec/User.tsp, updated TypeSpec/main.tsp

# List models
kq typespec list
# → Name    File        Doc Refs
# → User    User.tsp    TZ-001
```

## Traceability Checks

### Basic Traceability

```bash
# Full report
kq check traceability
# → Table: Object, Type, Status, Links, Recommendation

# Orphans (TypeSpec without documentation)
kq check orphans
```

### Deep Coverage

```bash
# Full chain BFT → FRD → ADR → TZ → TypeSpec
kq check traceability-deep --deep

# Custom chain
kq check traceability-deep --deep --chain "bft adr tz"

# JSON for editor plugins
kq check traceability-deep --deep --json
```

Deep coverage matrix:
- ✅ covered — full chain ❌ broken — chain gap
- 👻 orphan — no incoming links 🕸️ stale — doc changed, code not updated

### Cross-Repo Scan (Code Anchors)

```bash
# Scan projects from the config for @doc-anchor
kq check scan

# Rebuild the graph before scanning
kq check scan --rebuild
```

When an orphan anchor is found (`@doc-anchor PushNotificationManager` without TypeSpec),
a TASK with instructions is created automatically.

### Stale Notifications

```bash
# All stale links
kq check notify

# Last 7 days
kq check notify --since 7d
```

### Example knowledge.toml with projects

```toml
knowledge_path = "."

[[projects]]
path = "../mobile-app"
label = "iOS App"

[[projects]]
path = "../backend"
label = "Backend"

[watcher]
debounce_secs = 600
ignore_patterns = []

[search]
chunk_size = 512
chunk_overlap = 64
max_results = 10
relevance_threshold = 0.7
```

## README Auto-generation

```bash
# Regenerate
kq readme
```

Generates between the `<!-- kq:start -->` / `<!-- kq:end -->` markers:

1. **Kanban Board** — tasks by status (todo/in_progress/review/done)
2. **Table of Contents** — document tree with clickable links

Updated automatically on `kq push` and `kq watch`.

## Task Management

```bash
kq task new --title "Implement search" --priority high
kq task list
kq task status TASK-001 --set in_progress
kq task show TASK-001
kq task search "search"
```

## Search

```bash
# Full-text (FTS5, always available)
kq search "system architecture"

# Vector (sqlite-vec + Candle, when a model is available)
kq search --vector "semantic search"

# Hybrid (default)
kq search "database" --limit 20
```

## Watcher / Daemon

```bash
# Auto-commit changes
kq watch

# Custom debounce
kq watch --debounce-secs 120

# Trace daemon: watcher + auto-trace on every cycle
kq watch --trace
```

## Push

```bash
kq push                          # pull --rebase → readme-gen → push
kq push --no-readme              # skip README generation
kq push --dry-run                # show changes without pushing
```

## Documentation in Code (@doc-anchor)

Developers annotate code in their projects:

```swift
// @doc-anchor SecureTokenStorage
// @see docs://architecture/ADR-001.md
class KeychainStorage: TokenStorable { }
```

```go
// @doc-anchor AuthServiceImpl
func ValidateToken(token string) bool { }
```

When scanning, kq finds these annotations and builds the graph:
`BFT-001 → ADR-001 → TZ-001 → TypeSpec → @doc-anchor → code`

Orphan anchors (anchor exists, no TypeSpec) → task created automatically.

## Conflict Resolution

```bash
kq conflict list
kq conflict show docs/architecture.md
kq conflict resolve docs/architecture.md --ours
kq conflict resolve docs/architecture.md --theirs
```

## AI Assistance

```bash
kq ask "How is the architecture organized?"
kq ask "Describe the User model" --insert docs/user-model.md
```

Requires an LLM provider in `knowledge.toml` (Ollama/OpenAI/Anthropic).

## Code Quality

```bash
just lint        # clippy
just fmt         # fmt --check
just fmt-fix     # fmt
just check       # build + lint + test + fmt
```

## CI

```yaml
# On every push/PR — build + clippy + test for macOS/Linux/Windows
.github/workflows/ci.yml

# On tag v* — release build for 3 platforms + GitHub Release
.github/workflows/release.yml
```

## Installation

```bash
cargo install --path kq-cli
kq --help
```

Or download a binary from [GitHub Releases](https://github.com/bimawa/KnowledgeQuery).

## Project Structure

```
kq-cli/            # CLI entry point (clap)
kq-core/           # Business logic
├── check.rs       # Traceability, orphans, deep coverage
├── code_anchor.rs # @doc-anchor scanner
├── db.rs          # SQLite: files, trace_nodes, trace_links, code_anchors
├── docs.rs        # DocType, DocNode, front-matter parser
├── git.rs         # Git operations
├── readme_gen.rs  # README generator (Kanban + TOC)
├── task.rs        # CRDT tasks
├── typespec.rs    # TypeSpec models
└── watcher.rs     # File watcher
kq-embeddings/     # Candle + all-MiniLM-L6-v2
kq-llm/            # Ollama, OpenAI, Anthropic providers
kq-config/         # knowledge.toml parsing
```

## Knowledge Repository Structure

```
<project>/
├── .kq/
│   ├── knowledge.toml      # config
│   ├── knowledge.db        # SQLite: FTS + vectors + trace graph
│   ├── events/             # CRDT task events
│   └── templates/          # document templates
├── users/                  # users (username.md)
├── docs/                   # documentation (8 categories)
├── tasks/                  # tasks (TASK-NNN.md)
├── TypeSpec/               # TypeSpec models
└── README.md               # auto-Kanban + Table of Contents
```

## Architecture: CRDT Events

Tasks use CRDT events instead of overwrites:

```
create       → .kq/events/TASK-001/20260709T100000Z-create.md
assign alex  → .kq/events/TASK-001/20260709T110000Z-assign-alex.md
move review  → .kq/events/TASK-001/20260709T120000Z-move-review.md
```

**Why:** no merge conflicts, status is computed by replay,
readable git diff, parallel work without collisions.

### Example

```text
Alice: kq task assign TASK-001 alex  → ...assign-alex.md
Bob:   kq task assign TASK-001 maria → ...assign-maria.md
```

After a rebase, both event files end up side by side; last-writer-wins
is resolved by timestamp — no merge conflict.

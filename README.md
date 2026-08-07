# kqs — Git-native Knowledge Platform

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
./target/release/kqs <command>
```

### Test

```bash
just test
cargo test --workspace -- --nocapture
```

## Dual Mode

kqs runs in two contexts:

| Mode | Description | Commands |
|------|-------------|----------|
| **Dev** (`--dev`) | Inside a developer's project. Reads code, does not write to docs | scan, check, watch |
| **Doc** (`--doc`) | Inside a knowledge repository. Full access | everything + doc new, typespec, readme |

```bash
# Explicit
kqs --dev check scan
kqs --doc doc new adr "Title"

# Auto-detect: if docs/ + TypeSpec/ exist → doc mode
kqs check traceability

# Via env
KQS_MODE=dev kqs check scan
```

In dev mode, doc-only commands (`kqs doc new`, `kqs typespec new`, `kqs readme`)
fail with an explanatory error.

## Initialization

```bash
# Create a knowledge repository
kqs init
kqs init --path ~/my-knowledge --remote https://github.com/user/knowledge.git
```

## Working with Documents

```bash
# List templates
kqs doc template --list

# Create a document
kqs doc new adr "Database Choice"      # → docs/03-architecture/adr-001-*.md
kqs doc new tz "API Gateway Design"    # → docs/04-technical-design/tz-001-*.md
kqs doc new bft "Customer Portal"      # → docs/01-business-foundation/bft-001-*.md
kqs doc new frd "Sign-up"              # → docs/02-product-ux/frd-001-*.md

# Screen and User Flow
kqs screen "Login Screen"
kqs userflow "Onboarding Flow"

# List all documents
kqs doc list
```

## Document Types & the Document Chain

Every template in `.kqs/templates/` carries a specific semantic load — the type is chosen
by what question the document answers, not by preference. The types form a chain:
each next type refines the previous one, from business intent down to the data
specification that code implements.

| Type | Category | What this document does | Example |
|------|----------|-------------------------|---------|
| `idea` | Ideas | Unvalidated thought: problem statement, proposed solution, impact, effort estimate | "Push notifications will increase retention" |
| `bft` | Business Foundation | Business-functional foundation: objectives, stakeholders, scope (in/out). The root of the chain — what the business wants and why | BFT-001 "User Authentication" |
| `brd` | Business Foundation | Business requirements: what the business needs, priorities, assumptions, constraints, acceptance criteria | BR-001: "Payments must support card and SBP" |
| `frd` | Product & UX | Functional requirements: how the system behaves — UI, business rules, data, error handling | FR-002: "Login form must validate email format" |
| `nfr` | Business Foundation | Non-functional requirements: performance, security, availability, scalability | "p95 < 300 ms; TLS everywhere" |
| `user_story` | Product & UX | User story: As a / I want / so that, acceptance criteria | "As a user, I want to reset my password so that I can regain access" |
| `adr` | Architecture | Recorded architectural decision: context, decision, consequences, alternatives considered | ADR-001: "Store JWT in Keychain" |
| `rfc` | Architecture | Proposal for discussion before committing: motivation, detailed design, alternatives, open questions | RFC: "Migrate REST → GraphQL" |
| `tz` | Technical Design | Technical design: architecture, API, data model, implementation plan, testing strategy | TZ-001: "API Gateway design" |
| `screen` | Technical Design | Single screen: layout, components, states (loading/empty/error), interactions | "Login screen" |
| `userflow` | Technical Design | End-to-end flow: steps, decision points, error paths, success criteria | "Onboarding flow" |
| `glossary` | Glossary | Term definition, context, related terms | "Idempotency" |
| `typespec` | TypeSpec | Data specification (TypeSpec model) — the terminal link before code | `TypeSpec/User.tsp` |

### The Document Chain

Create documents in this order — each type flows from the previous one:

```
idea → bft → brd/frd/nfr → adr → rfc → tz → typespec → code
```

1. **idea** — a thought worth exploring. Once validated, formalize it into a `bft`.
2. **bft** — the root of the chain: what the business wants and why (objectives, stakeholders, scope). Example: "User Authentication".
3. **brd / frd / nfr** — derived from the `bft`: business, functional and non-functional requirements that detail it.
4. **adr** — an architectural decision made to satisfy those requirements: context, decision, consequences.
5. **rfc** — a proposal for a significant change, discussed before it becomes a final design.
6. **tz** — the technical design that fixes the decision in concrete terms: architecture, API, data model.
7. **typespec** — the data specification: models the design as checkable `TypeSpec` contracts.
8. **code** — implements the specification, linked back via `@doc-anchor`.

`kqs check traceability-deep` verifies the chain is complete (`BFT → ADR → TZ → TypeSpec`),
flagging any missing link.

## TypeSpec — Data Specifications

```bash
# Create a model
kqs typespec new User
# → TypeSpec/User.tsp, updated TypeSpec/main.tsp

# List models
kqs typespec list
# → Name    File        Doc Refs
# → User    User.tsp    TZ-001
```

## Traceability Checks

### Basic Traceability

```bash
# Full report
kqs check traceability
# → Table: Object, Type, Status, Links, Recommendation

# Orphans (TypeSpec without documentation)
kqs check orphans
```

### Deep Coverage

```bash
# Full chain BFT → FRD → ADR → TZ → TypeSpec
kqs check traceability-deep --deep

# Custom chain
kqs check traceability-deep --deep --chain "bft adr tz"

# JSON for editor plugins
kqs check traceability-deep --deep --json
```

Deep coverage matrix:
- ✅ covered — full chain ❌ broken — chain gap
- 👻 orphan — no incoming links 🕸️ stale — doc changed, code not updated

### Cross-Repo Scan (Code Anchors)

```bash
# Scan projects from the config for @doc-anchor
kqs check scan

# Rebuild the graph before scanning
kqs check scan --rebuild
```

When an orphan anchor is found (`@doc-anchor PushNotificationManager` without TypeSpec),
a TASK with instructions is created automatically.

### Stale Notifications

```bash
# All stale links
kqs check notify

# Last 7 days
kqs check notify --since 7d
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
kqs readme
```

Generates between the `<!-- kqs:start -->` / `<!-- kqs:end -->` markers:

1. **Kanban Board** — tasks by status (todo/in_progress/review/done)
2. **Table of Contents** — document tree with clickable links

Updated automatically on `kqs push` and `kqs watch`.

## Task Management

```bash
kqs task new --title "Implement search" --priority high
kqs task list
kqs task status TASK-001 --set in_progress
kqs task show TASK-001
kqs task search "search"
```

## Search

```bash
# Full-text (FTS5, always available)
kqs search "system architecture"

# Vector (sqlite-vec + Candle, when a model is available)
kqs search --vector "semantic search"

# Hybrid (default)
kqs search "database" --limit 20
```

## Watcher / Daemon

```bash
# Auto-commit changes
kqs watch

# Custom debounce
kqs watch --debounce-secs 120

# Trace daemon: watcher + auto-trace on every cycle
kqs watch --trace
```

## Push

```bash
kqs push                          # pull --rebase → readme-gen → push
kqs push --no-readme              # skip README generation
kqs push --dry-run                # show changes without pushing
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

When scanning, kqs finds these annotations and builds the graph:
`BFT-001 → ADR-001 → TZ-001 → TypeSpec → @doc-anchor → code`

Orphan anchors (anchor exists, no TypeSpec) → task created automatically.

## Conflict Resolution

```bash
kqs conflict list
kqs conflict show docs/architecture.md
kqs conflict resolve docs/architecture.md --ours
kqs conflict resolve docs/architecture.md --theirs
```

## AI Assistance

```bash
kqs ask "How is the architecture organized?"
kqs ask "Describe the User model" --insert docs/user-model.md
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

# On tag v* — publish all workspace crates to crates.io
.github/workflows/publish.yml
```

## Installation

```bash
# From crates.io (installs the `kqs` binary)
cargo install kqs

# From source
cargo install --path kqs

kqs --help
```

> **Note on the name:** the crates.io package [`kq`](https://crates.io/crates/kq)
> is an unrelated KDL query tool. Our package is `kqs` and installs the `kqs`
> binary — no conflict with anything on crates.io.

Or download a binary from [GitHub Releases](https://github.com/bimawa/KnowledgeQuery).

## Publishing

Workspace crates are published to crates.io from a `v*` tag:

1. `just bump-version 0.2.0` — bump the single workspace version in `Cargo.toml`
2. Commit and push the tag `v0.2.0` (must match the workspace version)
3. CI runs tests, then publishes `kq-config` → `kq-llm` → `kq-embeddings` → `kq-core` → `kqs` in dependency order
4. `just publish-check` — local pre-flight: dry-run publish of the leaf crates and a package file audit of the dependents

Requires the `CARGO_REGISTRY_TOKEN` secret in GitHub repository settings.

## Project Structure

```
kqs/               # CLI entry point (clap)
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
├── .kqs/
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
create       → .kqs/events/TASK-001/20260709T100000Z-create.md
assign alex  → .kqs/events/TASK-001/20260709T110000Z-assign-alex.md
move review  → .kqs/events/TASK-001/20260709T120000Z-move-review.md
```

**Why:** no merge conflicts, status is computed by replay,
readable git diff, parallel work without collisions.

### Example

```text
Alice: kqs task assign TASK-001 alex  → ...assign-alex.md
Bob:   kqs task assign TASK-001 maria → ...assign-maria.md
```

After a rebase, both event files end up side by side; last-writer-wins
is resolved by timestamp — no merge conflict.

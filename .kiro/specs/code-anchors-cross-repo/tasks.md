# Implementation Tasks: Code Anchors Phase 2 — Bidirectional Monitor

## 1. Orphan-to-Task (check.rs)

**Effort:** M  
**Req:** B.1  
**Files:** `kq-core/src/check.rs`

При сканировании (`kq check scan`) для каждого орфан-анкора создавать TASK в `.kq/events/`.

**Acceptance:**
- `create_orphan_task(anchor_name, file_path, repo_path)` создаёт CRDT-событие `.kq/events/TASK-NNN/`
- TASK имеет frontmatter: title из anchor, priority P1, assignee пустой
- TASK body содержит: где найден anchor, что нужно сделать (TypeSpec + doc)
- Если задача для этого anchor уже существует — не дублировать

## 2. kq check notify (check.rs)

**Effort:** S  
**Req:** A  
**Files:** `kq-core/src/check.rs`, `kq-cli/src/main.rs`

Команда `kq check notify --since <period>` — stale-уведомления.

**Acceptance:**
- `notify_stale(knowledge_path, since)` выбирает stale-связи из trace_links
- Фильтр `--since 7d` / `--since 2026-07-01` / без флага = все stale
- Вывод: doc → type → affected code anchors → файлы в проектах
- Если stale нет — сообщение "No stale links found"

## 3. --orphans flag (check.rs)

**Effort:** S  
**Req:** B.2  
**Files:** `kq-core/src/check.rs`, `kq-cli/src/main.rs`

Флаг `--orphans` для `kq check traceability-deep`, показывающий только орфаны.

**Acceptance:**
- `kq check traceability-deep --deep --orphans` — только ❌ и 🕸️ секции
- `kq check traceability-deep --deep` (без --orphans) — полный отчёт
- Формат: как в дизайне, три секции (orphans, stale, traced)

## 4. Build and Test

**Effort:** S  
**Files:** все

- `cargo test --workspace` — 166+ тестов проходят
- Ручная проверка на TestDocWorld
- `kq check scan` → orphan → задача создана
- `kq check notify --since 1d` → stale links (если есть)

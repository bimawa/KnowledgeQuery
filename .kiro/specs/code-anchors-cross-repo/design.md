# Design: Code Anchors & Cross-Repos Scanning

> **Spec:** `code-anchors-cross-repo`
> **Language:** ru
> **Status:** Draft

## Overview

Добавить модуль `code_anchor` в kq-core для сканирования внешних репозиториев, поиска `@doc-anchor` и `@see` аннотаций, и интеграции с trace-графом.

## Architecture

```
kq-core/src/code_anchor.rs  (новый модуль)
  ├── CodeAnchor struct
  ├── scan_project() — обход файлов проекта
  ├── scan_file() — парсинг одного файла на анкоры
  ├── resolve_anchors() — матчинг анкоров с документами
  └── update_anchor_db() — запись в code_anchors таблицу

kq-config/src/lib.rs
  └── ProjectConfig.scan_patterns: Vec<String>  (новое поле)

kq-core/src/check.rs
  └── scan_projects() — вызов code_anchor::scan_project для всех проектов

kq-cli/src/main.rs
  └── kq check --scan флаг
```

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS code_anchors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anchor TEXT NOT NULL,
    repo_path TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line_number INTEGER,
    anchor_type TEXT NOT NULL,   -- doc_anchor | see_ref
    target_doc TEXT,             -- для @see docs://... (парсится путь)
    file_hash TEXT,
    last_seen TEXT NOT NULL,
    UNIQUE(anchor, file_path, line_number)
);

CREATE INDEX IF NOT EXISTS idx_code_anchors_anchor ON code_anchors(anchor);
CREATE INDEX IF NOT EXISTS idx_code_anchors_type ON code_anchors(anchor_type);
```

## CodeAnchor Scanner

```rust
#[derive(Debug, Clone)]
pub struct CodeAnchor {
    pub anchor: String,         // имя анкора
    pub repo_path: String,
    pub file_path: String,
    pub line_number: u32,
    pub anchor_type: AnchorType,
    pub target_doc: Option<String>,  // для @see
    pub file_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnchorType {
    DocAnchor,   // @doc-anchor <name>
    SeeRef,      // @see docs://<path>
}
```

### Парсинг одного файла

```rust
// Комментарий-детектор по расширению
fn comment_patterns(ext: &str) -> [(Regex, fn(&str) -> Vec<AnchorCandidate>)];

// Поддерживаемые:
// .rs, .go, .swift, .kt, .java, .ts, .js: // @doc-anchor <name>
// .py, .rb, .sh, .yaml, .yml:              # @doc-anchor <name>
// .html, .xml, .md:                         <!-- @doc-anchor <name> -->
// .c, .cpp, .h, .hpp:                       // or /* @doc-anchor <name> */

pub fn scan_file(path: &Path, repo_path: &str) -> Result<Vec<CodeAnchor>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let content = fs::read_to_string(path)?;
    let hash = sha2_hex(&content);
    let mut anchors = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(anchor) = extract_doc_anchor(line, ext) {
            anchors.push(CodeAnchor { ... });
        }
        if let Some(target) = extract_see_ref(line, ext) {
            anchors.push(CodeAnchor { anchor_type: SeeRef, target_doc: Some(target), ... });
        }
    }

    Ok(anchors)
}
```

### Обход проекта

```rust
pub fn scan_project(config: &ProjectConfig, knowledge_path: &Path) -> Result<ScanSummary> {
    let patterns = if config.scan_patterns.is_empty() {
        DEFAULT_SCAN_PATTERNS.to_vec()
    } else {
        config.scan_patterns.clone()
    };

    let mut total_anchors = 0;
    let mut total_files = 0;

    for entry in walkdir::WalkDir::new(&config.path)
        .into_iter()
        .filter_entry(|e| !is_ignored(e.path(), &config.ignore_patterns))
    {
        let entry = entry?;
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        if !matches_any_pattern(path, &patterns) { continue; }

        let anchors = scan_file(path, &config.path.to_string_lossy())?;
        if !anchors.is_empty() {
            total_anchors += anchors.len();
            total_files += 1;
            update_anchor_db(&anchors)?;
        }
    }

    Ok(ScanSummary { total_files, total_anchors })
}
```

## Конфиг

```toml
# knowledge.toml (расширение)
[knowledge]
# ... существующие поля

[[projects]]
path = "../my-app-ios"
label = "iOS App"
scan_patterns = ["**/*.swift", "**/*.kt"]

[[projects]]
path = "../my-app-backend"
label = "Backend"
# используются DEFAULT_SCAN_PATTERNS
```

## CLI

```bash
# Сканировать все проекты из конфига
kq check --scan

# Сканировать конкретный проект
kq check --scan --project "iOS App"

# Сухие флаги работают вместе
kq check traceability --deep --scan
```

## Интеграция с trace graph

При сканировании:
1. Для каждого `@doc-anchor SecureTokenStorage` — ищем документы, у которых в `code_anchors` есть `SecureTokenStorage`
2. Если найден — создаём `implements` link в trace_links от типа (typespec → код)
3. Для каждого `@see docs://architecture/ADR-0002.md` — проверяем существование документа
4. Если документ есть — создаём `references` link
5. Если нет — выводим warning и создаём `broken` link

## Module Changes

| Файл | Изменение |
|------|-----------|
| `kq-core/src/code_anchor.rs` | Новый модуль: сканер, парсинг, база |
| `kq-core/src/check.rs` | + `scan_projects()`, + `--scan` флаг |
| `kq-core/src/db.rs` | + `create_code_anchor_schema()`, + `code_anchors` CRUD |
| `kq-config/src/lib.rs` | + `ProjectConfig.scan_patterns` |
| `kq-cli/src/main.rs` | + `--scan` флаг для `kq check` |

---

## Bidirectional Monitor (Phase 2)

kq **не генерирует код и не создаёт TypeSpec**. kq — монитор целостности.
Его задача: обнаружить орфан и уведомить. Человек сам создаёт TypeSpec, сам пишет код, сам дёргает `kq typespec new` когда надо.

### Направление A: Docs → Code (Forward)

Документ изменился (revision +1). kq проверяет: какие `code_anchors` ссылаются на этот док?
Если есть — связи становятся `stale` → уведомление разработчику и tech lead.

```bash
kq check notify --since 7d
# → ADR-001 revision 2→3, stale links:
#     SecureTokenStorage → mobile-app/KeychainDriver.swift
#     AuthServiceImpl   → backend/auth_service.go
```

Разработчик видит: "ADR-001 изменился, мой код может не соответствовать". Решает сам — обновлять код или нет.

### Направление B: Code → Docs (Backward)

```bash
kq check scan
# → @doc-anchor PushNotificationManager ← орфан
#   Нет TypeSpec модели, нет документа
```

kq создаёт **задачу** (TASK-NNN.md):

```markdown
---
title: "Create TypeSpec + doc for PushNotificationManager"
priority: P1
---
# TASK-006: Link PushNotificationManager

@dev Найден @doc-anchor PushNotificationManager в:
  external-projects/mobile-app/.../PushManager.swift

Что нужно:
- [ ] Создать TypeSpec модель: kq typespec new PushNotificationManager
- [ ] Создать TZ-документ: kq doc new tz "Push Notification Service"
- [ ] Проверить trace: kq check traceability
```

Tech lead видит в README (Kanban) новую задачу. Назначает разработчика. После создания TypeSpec и документа — `kq check traceability-deep` покажет что цепь замкнута.

### Двунаправленный ревьювер (дэшборд)

```bash
kq check traceability-deep --deep --orphans
# → Секции:
#
# ❌ Orphan anchors (есть в коде, нет в docs):
#   PushNotificationManager (mobile-app)
#   RateLimiter (backend)
#
# 🕸️ Stale links (док изменился, код не обновлён):
#   SecureTokenStorage → KeychainDriver.swift (ADR-001 rev 2→3)
#
# ✅ Fully traced:
#   BFT-001 → FRD-001 → ADR-001 → TZ-001 → User.tsp → AuthService.swift
```

Это и есть тот самый отчёт для tech lead. Одна команда — видно всё:
- что не задокументировано (орфаны в коде)
- что устарело (изменения docs без обновления кода)
- что в порядке

### Дорожная карта Phase 2

| Шаг | Что делаем | Результат |
|-----|-----------|-----------|
| 1 | `kq check notify` — команда уведомлений | stale-детекция, отфильтрованная по дате |
| 2 | `kq check scan` → создание TASK при орфане | Авто-создание задачи в `.kq/events/` |
| 3 | `kq check traceability-deep --orphans` | Сводка для tech lead |
| 4 | CI-сервис (отдельно) | Периодический scan + notify по Slack/email |

### Изменения в модулях

| Файл | Изменение |
|------|-----------|
| `kq-core/src/check.rs` | + `notify_stale()`, + `create_orphan_task()` |
| `kq-cli/src/main.rs` | + `kq check notify --since <period>` |

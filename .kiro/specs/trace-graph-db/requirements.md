# Requirements: Trace Graph Database & Deep Coverage

> **Spec:** `trace-graph-db`
> **Language:** ru
> **Status:** Draft

## Introduction

Заменить текущий одноразовый парсинг `@doc`-ссылок в `check.rs` на постоянный SQLite-граф связей.
Граф позволяет:
- Deep coverage: проверка полных цепочек (BFT→ADR→TZ→code) вместо одной ссылки
- Stale-детекция: обнаружение устаревших связей при изменении документов
- Быстрые запросы: один SQL вместо сканирования всех .md файлов
- Фундамент для Wave 2: code anchors и cross-repo scan

## Boundary Context

- **In scope**: SQLite-таблицы для графа, парсинг Front Matter, deep coverage алгоритм, обновлённый `kq check traceability`
- **Out of scope**: Code anchors (`@doc-anchor`), cross-repo scan, revision tracking, trace daemon — это Wave 2
- **Adjacent**: Существующий `check.rs` остаётся, но будет переписан для работы через граф

## Requirements

### REQ-001: Trace Graph Tables

**Objective:** As a пользователь, I want связи между документами храниться в SQLite, so что запросы трассировки выполняются за миллисекунды, а не за сканирование всех .md файлов.

#### Acceptance Criteria
1. When `kq init` или первый `kq check traceability`, kq shall создать таблицы `trace_links` и `trace_nodes` в `.kq/knowledge.db`.
2. The `trace_nodes` table shall содержать: `node_id TEXT PK`, `node_type TEXT` (bft, adr, tz, frd, nfr, rfc, brd, typespec), `title TEXT`, `file_path TEXT`, `revision INTEGER DEFAULT 1`, `status TEXT DEFAULT 'active'`, `created_at TEXT`, `updated_at TEXT`.
3. The `trace_links` table shall содержать: `link_id INTEGER PK AUTO`, `source_id TEXT FK→trace_nodes`, `target_id TEXT FK→trace_nodes`, `link_type TEXT` (covers, needs, references, implements), `status TEXT DEFAULT 'valid'`, `created_at TEXT`.
4. When документ удалён из файловой системы, kq shall пометить соответствующий node как `status: removed`, не удалять сразу (для аудита).
5. The SQLite schema shall быть версионирована (таблица `schema_version` с номером миграции).

### REQ-002: Front Matter Parser Upgrade

**Objective:** As a разработчик, I want kq парсить все поля Front Matter документов, so что граф строится автоматически из YAML-метаданных.

#### Acceptance Criteria
1. When kq парсит .md файл, kq shall извлекать из Front Matter: `id`, `title`, `status`, `type` (из пути файла), `revision` (если есть), `bft_refs`, `rfc_refs`, `needs`, `covers`.
2. The kq shall распознавать тип документа по пути: `docs/01-business-foundation/BFT-*.md` → `bft`, `docs/03-architecture/ADR-*.md` → `adr`, `docs/04-technical-design/TZ-*.md` → `tz` и т.д.
3. The kq shall также парсить inline-ссылки `@doc XXX-NNN` в теле .md и .tsp файлов (как сейчас).
4. When поле `needs: ["ADR", "TZ"]` указано в BFT-документе, kq shall создать link_type=`needs` от BFT к ADR/TZ.
5. When поле `covers: ["BFT-001"]` указано в TZ-документе, kq shall создать link_type=`covers` от TZ к BFT-001.
6. The kq shall при обновлении документа перезаписывать его node в графе и пересчитывать все связи.

### REQ-003: Deep Coverage Algorithm

**Objective:** As a QA-инженер, I want kq проверять полные цепочки трассировки, so что видеть не только прямые ссылки, но и разрывы в иерархии требований.

#### Acceptance Criteria
1. When пользователь выполняет `kq check traceability --deep`, kq shall построить транзитивное замыкание графа от выбранного типа до terminating-типа.
2. The default chain shall быть: `bft → (brd|frd|nfr) → adr → tz → typespec`. Каждый шаг опционален — граф проверяет наличие хотя бы одного пути.
3. When звено цепочки отсутствует (например BFT→ADR есть, а ADR→TZ нет), kq shall пометить цепочку как `broken` с указанием разрыва.
4. The terminating types (не требуют покрытия) shall быть: `typespec`, `idea`, `glossary`.
5. When пользователь выполняет `kq check traceability --chain "bft tz"`, kq shall проверить только указанную цепочку.
6. When все цепочки полные, kq shall вывести матрицу с колонками: Node, Type, Chain Status, Coverage %, Missing Links.

### REQ-004: Traceability Report Upgrade

**Objective:** As a tech lead, I want отчёт трассировки показывать статусы связей, so что понимать, какие документы актуальны, а какие требуют внимания.

#### Acceptance Criteria
1. When `kq check traceability` выполняется, kq shall вывести таблицу со статусами для каждого узла: `✅ covered` (полная цепочка), `⚠️ partial` (цепочка неполная), `❌ orphan` (нет входящих связей), `💀 dangling` (исходящие ссылки в никуда), `🕸️ stale` (ревизия не совпадает).
2. When есть `stale` связи, kq shall показать: какой документ изменился, какие связи требуют проверки, timestamp последнего изменения.
3. When пользователь выполняет `kq check traceability --json`, kq shall вывести весь граф в JSON для интеграции с плагинами редакторов.
4. The kq shall группировать отчёт по типам: BFT, ADR, TZ, и т.д. с под-итогами для каждого типа.

### REQ-005: Incremental Re-index

**Objective:** As a разработчик, I want kq обновлять граф только для изменённых файлов, so что повторные проверки не сканируют всю файловую систему.

#### Acceptance Criteria
1. When `kq check traceability` запущен повторно, kq shall проверять hash последнего коммита Git для knowledge-репозитория.
2. When Git hash не изменился, kq shall использовать кеш графа без пересканирования.
3. When Git hash изменился, kq shall найти изменённые файлы через `git diff --name-only <last_hash> HEAD` и переиндексировать только их.
4. The kq shall хранить last_indexed_commit в `schema_version` таблице.
5. When файл удалён из Git (не просто изменён), kq shall пометить node как `removed` и очистить его связи.

### REQ-006: Backward Compatibility

**Objective:** As a пользователь существующего kq, I want старые команды продолжать работать, so что обновление не ломает текущий workflow.

#### Acceptance Criteria
1. The `kq check traceability` без флагов shall работать как раньше (выводить таблицу), но данные теперь из графа, а не из одноразового парсинга.
2. The `kq check orphans` без флагов shall работать как раньше.
3. When `.kq/knowledge.db` ещё не содержит таблиц графа (старая версия), kq shall автоматически создать их при первом `kq check`.
4. The формат вывода таблицы (терминал) shall остаться совместимым — те же колонки, те же эмодзи.
5. When пользователь удаляет `.kq/knowledge.db` и запускает `kq check`, kq shall пересобрать граф с нуля.

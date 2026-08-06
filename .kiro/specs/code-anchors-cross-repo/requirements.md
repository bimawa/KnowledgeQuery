# Requirements: Code Anchors & Cross-Repos Scanning

> **Spec:** `code-anchors-cross-repo`
> **Language:** ru
> **Status:** Draft

## Introduction

Добавить возможность аннотировать исходный код метками `@doc-anchor <name>` и `@see docs://<path>`, а затем сканировать внешние репозитории проектов для построения полного графа трассировки документ → код.

**Зачем:** Сейчас kq знает только о связях внутри docs-репозитория. Чтобы отвечать на вопрос «какой код реализует это требование?», нужно сканировать исходники проектов.

## Boundary Context

- **In scope**: `@doc-anchor` и `@see` парсинг в исходниках, `kq check --scan`, `code_anchors` конфиг, интеграция с trace_graph
- **Out of scope**: Pull request проверки, авто-исправление анкоров, push уведомлений в code-repos (см. P-009)
- **Adjacent**: Зависит от `trace-graph-db` (таблицы уже созданы, функция `upsert_trace_node` готова)

## Requirements

### REQ-001: Code Anchor Scanner

**Objective:** As a разработчик, I want kq находить `@doc-anchor <name>` в исходниках, so что код автоматически связывается с документацией.

#### Acceptance Criteria
1. When kq сканирует файл, kq shall искать паттерн `@doc-anchor <name>` в комментариях всех поддерживаемых языков.
2. The kq shall поддерживать: `// @doc-anchor <name>` (Rust, Go, Swift, C, C++, Kotlin, Java, TypeScript), `# @doc-anchor <name>` (Python, Ruby, Shell, YAML), `<!-- @doc-anchor <name> -->` (HTML, XML, Markdown внутри комментариев), `/* @doc-anchor <name> */` (C, C++, Swift, Kotlin, Java).
3. The kq shall также искать `@see docs://<type>/<id>.md` — обратные ссылки из кода на документацию.
4. When найден anchor, kq shall сохранить его в новой таблице `code_anchors` в `.kq/knowledge.db`.

### REQ-002: Cross-Repo Scan Command

**Objective:** As a tech lead, I want команду `kq check --scan` для сканирования всех проектов из конфига, so что граф трассировки строится автоматически.

#### Acceptance Criteria
1. When пользователь выполняет `kq check --scan`, kq shall прочитать список `projects` из `knowledge.toml`.
2. For each project, kq shall рекурсивно обойти все файлы (кроме `.git`, `node_modules`, `target`, `.obsidian`, и паттернов из `ignore_patterns`).
3. The kq shall парсить найденные `@doc-anchor` и `@see` во всех файлах проекта.
4. When anchor совпадает с `code_anchors` в Front Matter документа, kq shall создать связь `implements` в trace_links.
5. When `@see docs://` ссылается на несуществующий документ, kq shall вывести предупреждение.
6. The kq shall кешировать результаты сканирования в `code_anchors` таблице и обновлять только изменённые файлы (по mtime/hash).

### REQ-003: Code Anchors Table

**Objective:** As a разработчик, I want новая SQLite-таблица `code_anchors` для хранения найденных анкоров, so что сканирование происходит один раз, а запросы — мгновенно.

#### Acceptance Criteria
1. The `code_anchors` table shall содержать: `id INTEGER PK AUTO`, `anchor TEXT NOT NULL`, `repo_path TEXT NOT NULL`, `file_path TEXT NOT NULL`, `line_number INTEGER`, `anchor_type TEXT` (doc_anchor | see_ref), `target_doc TEXT` (для @see), `file_hash TEXT`, `last_seen TEXT`.
2. The table shall иметь UNIQUE constraint на `(anchor, file_path, line_number)`.
3. When файл не изменился (file_hash тот же), kq shall не перезаписывать запись.
4. When файл удалён, kq shall удалить соответствующие записи.

### REQ-004: Config Extension

**Objective:** As a пользователь, I want настраивать сканирование через `knowledge.toml`, so что контролировать какие проекты и паттерны сканировать.

#### Acceptance Criteria
1. The `ProjectConfig` shall получить новое поле `scan_patterns: Vec<String>` (какие файлы сканировать, по умолчанию `["**/*.rs", "**/*.swift", "**/*.kt", "**/*.go", "**/*.py", "**/*.ts", "**/*.js"]`).
2. The watcher config `ignore_patterns` shall применяться и к сканированию.
3. When `projects` пустой, `kq check --scan` shall вывести сообщение и завершиться успешно.

### REQ-005: Integration with Trace Graph

**Objective:** As a QA-инженер, I want найденные code anchors показываться в отчёте `kq check traceability`, so что видно не только связи docs↔docs, но и docs↔code.

#### Acceptance Criteria
1. When пользователь выполняет `kq check traceability`, kq shall включить code anchors в матрицу трассировки.
2. The matrix shall показывать для каждого документа: сколько code anchors найдено, какие файлы реализуют требование.
3. When `--deep` флаг передан, kq shall проверять, что каждый `code_anchor` из Front Matter имеет соответствующий `@doc-anchor` в коде.
4. Orphan anchors (есть `@doc-anchor` в коде, но нет `code_anchors` в документе) показываются в отдельной секции.

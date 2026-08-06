# Conventions Steering: kq

## Repository Structure

### Два репозитория

```
VideoCatDoc/                    # корневой репозиторий проекта (пример)
├── README.md                   # АВТОГЕНЕРИРУЕТСЯ kq: канбан, статусы, ссылки
├── docs/                       # документация проекта рядом с кодом
└── src/

~/.knowledge/                   # knowledge-репозиторий (путь настраивается)
├── knowledge.toml              # конфиг kq
├── README.md                   # описание knowledge-репозитория
├── docs/                       # база знаний (markdown)
├── tasks/                      # задачи (TASK-NNN.md)
└── .kq/                        # sqlite-vec DB + кеш модели
```

### Задачи

- Один файл = одна задача: `tasks/TASK-001.md`
- Frontmatter (YAML):
  ```yaml
  ---
  title: "Название задачи"
  status: todo | in_progress | review | done
  assignee: ~ | @username
  priority: P0 | P1 | P2 | P3
  created: 2026-07-09
  updated: 2026-07-09
  ---
  ```
- Нумерация — сквозная, автоинкремент

### README-блок (автогенерация)

```markdown
<!-- kq:start -->
## 🎯 Статус
<!-- kq генерирует: канбан, статистику, ссылки на docs -->
<!-- kq:end -->
```

- kq пишет ТОЛЬКО между маркерами `<!-- kq:start -->` / `<!-- kq:end -->`
- Ручные правки вне маркеров неприкосновенны
- Если маркеров нет — kq НЕ трогает README (первый раз вставляет маркеры с пустым блоком)

## Git Workflow

- **Auto-commit (watcher):** `git add .` → `git commit -m "auto: ..."` (локально, без push)
- **Manual push:** `kqs push` → `git pull --rebase` → README-gen → `git add README.md` → `git commit --amend` → `git push`
- **No stash:** watcher не делает stash, только stage + commit
- **Ignored:** .git, node_modules, target, .obsidian, __pycache__

## Architecture Patterns

### Crate Layering

```
kqs    ──>  kq-core  ──>  kq-embeddings
                │              kq-llm
                └──>  kq-config
```

- kq-core — центральный крейт, вся бизнес-логика
- kqs — только парсинг аргументов и вывод, без логики
- kq-embeddings / kq-llm — изолированные крейты с чёткими интерфейсами (traits)
- kq-config — чтение конфига, никакой логики

### Watcher

- `notify` event stream → debounce 600s → `git add .` → `git commit`
- Работает только на knowledge-репозитории и проектном репозитории (из конфига)
- Игнорирует .git, .obsidian, скрытые файлы

### Search

- **FTS (всегда):** sqlite FTS5, всегда доступен, без дополнительных зависимостей
- **Vector (при наличии модели):** sqlite-vec + Candle, документы чанкуются (512 токенов)
- **Hybrid (по умолчанию):** FTS + vector с взвешенным ранжированием

## Error Handling

- Все Git-операции с проверкой кода возврата
- LLM-провайдер недоступен → graceful fallback с сообщением
- Модель эмбеддингов не скачана → `kqs search` с предупреждением, FTS работает
- Git-конфликт при push → exit code != 0, `kqs conflict` для разрешения

## Testing Rules

- Core логика — unit тесты (без файловой системы)
- Git операции — integration тесты с временными репозиториями
- CLI — snapshot тесты (вывод команд)
- Search — golden файлы с ожидаемыми результатами

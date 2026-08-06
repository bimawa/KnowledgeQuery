# Requirements Document

## Introduction
kq (Knowledge Query) — CLI-инструмент для управления знаниями и документацией в Git + markdown. Единый бинарник, без сервера и Web UI, offline-first поиск. Obsidian — интерфейс для чтения/редактирования.
Пользователи: разработчики (CLI), не-разработчики (Obsidian), tech leads (task management + README).

## Boundary Context
- **In scope**: CLI + background daemon (watcher); управление knowledge-репозиторием; авто-коммит и ручной push; FTS и векторный поиск; AI-чат (опционально); трекинг задач; README-генератор; multi-repo watcher; история изменений; обработка конфликтов.
- **Out of scope**: Web UI / REST API (Obsidian — UI); документооборот; гранулярные права доступа; мобильное приложение; real-time коллаборация; Obsidian-плагин.
- **Adjacent expectations**: Git должен быть установлен в системе; для векторного поиска требуется скачивание модели эмбеддингов (~80 МБ); для LLM-функций требуется внешний провайдер (Ollama/OpenAI/Anthropic) в конфиге.

### Структура проекта
```
<project>/
├── .kq/
│   ├── knowledge.toml      # конфиг kq
│   ├── db.sqlite           # FTS + векторная БД
│   ├── state.db            # SQLite — актуальное состояние задач/юзеров
│   ├── events/             # CRDT-события (операции в .md)
│   │   └── TASK-NNN/
│   │       └── YYYYMMDDTHHMMSSZ-<op>-<value>.md
│   ├── model-cache/        # кеш модели эмбеддингов
│   └── templates/          # шаблоны документов
├── users/                  # пользователи (username.md)
├── docs/                   # документация
├── tasks/                  # задачи (TASK-NNN.md)
├── TypeSpec/               # TypeSpec-модели
└── README.md               # автогенерация маркерного блока
```

## Requirements

### Requirement 1: Инициализация knowledge-репозитория
**Objective:** As a разработчик, I want инициализировать структуру knowledge-репозитория одной командой, so что начать работу без ручной настройки.

#### Acceptance Criteria
1. When пользователь выполняет `kq init --path <PATH>`, the kq shall создать директории `docs/`, `tasks/`, `.kq/` и файл `.kq/knowledge.toml` по указанному пути.
2. When `--path` не указан, the kq shall инициализировать текущую директорию как knowledge-репозиторий.
3. When путь уже инициализирован, the kq shall вывести ошибку и завершиться с ненулевым кодом, если не передан `--force`.
4. When передан `--remote <URL>`, the kq shall выполнить `git init`, затем `git remote add origin <URL>`.
5. The kq shall после создания структуры выполнить `git add .` и `git commit -m "init"`.
6. The kq shall создать `.kq/knowledge.toml` с дефолтными настройками, парсируемыми обратно через kq-config.

### Requirement 2: Файловый watcher с авто-коммитом
**Objective:** As a разработчик, I want watcher автоматически коммитить изменения в knowledge-репозитории, so что ни одно изменение не потеряно.

#### Acceptance Criteria
1. When пользователь выполняет `kq watch`, the kq shall запустить рекурсивное отслеживание файловой системы в knowledge-репозитории.
2. When файл создан, изменён или удалён, the kq shall записать событие и сбросить debounce-таймер.
3. When debounce-таймер (по умолчанию 600 секунд) достиг нуля, the kq shall выполнить `git add .` → `git commit -m "auto: YYYY-MM-DD HH:MM:SS"`.
4. While watcher активен, the kq shall игнорировать события в `.git/`, `.obsidian/`, `node_modules/`, `target/`, скрытых файлах.
5. When получен сигнал SIGINT или SIGTERM, the kq shall завершить watcher с корректным закрытием файловых дескрипторов.
6. The kq shall после успешного коммита вызвать переиндексацию FTS.
7. If Git-операция не удалась (lock-файл, права доступа), the kq shall записать ошибку в stderr и продолжить наблюдение.

### Requirement 3: Полнотекстовый поиск (FTS)
**Objective:** As a пользователь, I want искать по содержимому .md файлов, so что быстро находить нужную информацию без интернета.

#### Acceptance Criteria
1. When пользователь выполняет `kq search --fts <query>`, the kq shall выполнить FTS5-запрос к `.kq/knowledge.db`.
2. The kq shall после `kq init` создать SQLite-базу с таблицами `files` (id, path, content_hash, content) и виртуальной таблицей FTS5.
3. When файл добавлен или изменён, the kq shall обновить запись в БД (по хэшу SHA256).
4. The kq shall в результатах поиска выводить: путь к файлу, строку контекста с совпадением, релевантность.
5. When пользователь выполняет `kq search` без флагов, the kq shall использовать FTS по умолчанию.
6. While модель эмбеддингов не скачана, the kq shall выполнять только FTS-поиск и выводить предупреждение о недоступности векторного поиска.

### Requirement 4: Векторный поиск (sqlite-vec + Candle)
**Objective:** As a пользователь, I want семантический поиск по смыслу, so что находить документы даже без точного совпадения слов.

#### Acceptance Criteria
1. When пользователь выполняет `kq search` и модель эмбеддингов скачана, the kq shall выполнить гибридный поиск (FTS + векторный) с взвешенным ранжированием.
2. When `kq search` запущен впервые, the kq shall скачать модель all-MiniLM-L6-v2 (~80 МБ) через hf-hub с прогресс-баром.
3. The kq shall разбивать документы на чанки (512 токенов с перекрытием) перед векторизацией.
4. Where модель скачана, the kq shall кешировать эмбеддинги в `.kq/` и обновлять только изменённые файлы.
5. The kq shall выполнять векторизацию на CPU через Candle, без CUDA.
6. If скачивание модели не удалось (нет сети), the kq shall вывести ошибку и предложить использовать FTS.

### Requirement 5: Команда push
**Objective:** As a разработчик, I want отправлять изменения в remote репозиторий, so что синхронизировать базу знаний с командой.

#### Acceptance Criteria
1. When пользователь выполняет `kq push`, the kq shall выполнить `git pull --rebase`.
2. When `git pull --rebase` успешен, the kq shall сгенерировать README (см. Requirement 9), затем `git add README.md` → `git commit --amend` → `git push`.
3. When возник Git-конфликт при rebase, the kq shall завершиться с ненулевым кодом и вывести инструкцию по разрешению конфликта через `kq conflict`.
4. When передан `--dry-run`, the kq shall показать, какие изменения будут отправлены, без фактического push.
5. When передан `--no-readme`, the kq shall пропустить генерацию README.

### Requirement 6: Обработка конфликтов
**Objective:** As a разработчик, I want инструменты для разрешения Git-конфликтов, so что быстро восстановить синхронизацию.

#### Acceptance Criteria
1. When пользователь выполняет `kq conflict`, the kq shall вывести список файлов с конфликтами.
2. When пользователь выполняет `kq conflict show <FILE>`, the kq shall отобразить содержимое конфликтующего файла с маркерами конфликта.
3. When пользователь выполняет `kq conflict resolve <FILE> --ours`, the kq shall разрешить конфликт в пользу локальной версии.
4. When пользователь выполняет `kq conflict resolve <FILE> --theirs`, the kq shall разрешить конфликт в пользу remote-версии.

### Requirement 7: Управление пользователями
**Objective:** As a tech lead, I want управлять пользователями команды, so что задачи имеют ответственных.

#### Acceptance Criteria
1. When пользователь выполняет `kq user create <username> [--name "Full Name"]`, the kq shall создать `users/<username>.md` с frontmatter (name, created, role).
2. When пользователь выполняет `kq user list`, the kq shall вывести всех пользователей с их именем и ролью.
3. When пользователь выполняет `kq user show <username>`, the kq shall показать профиль пользователя и его активные задачи.
4. The kq shall при создании задачи с `--assign <username>` проверять, что пользователь существует.

### Requirement 8: Управление задачами (CRDT-события)
**Objective:** As a tech lead, I want трекать прогресс команды без merge-конфликтов, so что несколько человек могут параллельно работать с задачами.

#### Acceptance Criteria
1. When пользователь выполняет `kq task new --title "..."`, the kq shall:
   - Создать `tasks/TASK-NNN.md` с title, priority, created в frontmatter (без status/assignee).
   - Создать событие `.kq/events/TASK-NNN/YYYYMMDDTHHMMSSZ-create.md` с операцией create.
2. The kq shall НЕ хранить status и assignee в frontmatter `tasks/TASK-NNN.md`. Состояние вычисляется из событий.
3. When пользователь выполняет `kq task assign TASK-NNN <username>`, the kq shall создать событие `...-assign-<username>.md` в `.kq/events/TASK-NNN/`.
4. When пользователь выполняет `kq task move TASK-NNN <status>`, the kq shall создать событие `...-move-<status>.md` в `.kq/events/TASK-NNN/`.
5. When пользователь выполняет `kq task desc TASK-NNN`, the kq shall открыть `tasks/TASK-NNN.md` в редакторе (обновление описания).
6. The kq shall нумеровать задачи сквозным автоинкрементом (TASK-001, TASK-002, ...).
7. The kq shall хранить локальное состояние задач в `.kq/state.db` (SQLite) для быстрого чтения. Состояние восстанавливается replay событий при pull.

#### Формат события
```markdown
---
task: TASK-142
op: assign
value: alex
timestamp: 2026-07-09T10:00:00Z
author: "Alex Smith"
---

Assigned **alex** to TASK-142.
```

#### Порядок событий
- Сортировка по timestamp в имени файла: `YYYYMMDDTHHMMSSZ-<op>-<value>.md`
- Если timestamp совпадает — LWW (Last Writer Wins) по имени файла
- Разные поля (assign + move) — независимы, merge без конфликта

#### Состояние задачи
```text
create       → status: todo,        assignee: —
assign alex  → status: todo,        assignee: alex
move review  → status: review,      assignee: alex
```

### Requirement 9: Sync (push/pull)
**Objective:** As a разработчик, I want синхронизировать изменения с командой, so что все видят актуальное состояние.

#### Acceptance Criteria
1. When пользователь выполняет `kq pull` или любую `kq task *` команду, the kq shall выполнить `git pull --rebase` (через jj/git).
2. After pull, the kq shall replay новые события из `.kq/events/` в `.kq/state.db`.
3. When пользователь выполняет `kq push`, the kq shall выполнить commit событий и push в remote.
4. The kq shall при rebase автоматически разрешать конфликты: новые файлы в `.kq/events/` не конфликтуют — у каждого уникальное имя.

### Requirement 10: README-генератор
**Objective:** As a tech lead, I want README проекта автоматически отражать статус задач, so что состояние команды видно на главной странице репозитория.

#### Acceptance Criteria
1. When `kq push` или `kq readme` выполняется, the kq shall сгенерировать Kanban-доску задач между маркерами `<!-- kq:start -->` и `<!-- kq:end -->` в README.md.
2. The kq shall не трогать содержимое README вне маркеров `<!-- kq:start -->` / `<!-- kq:end -->`.
3. If маркеров нет, the kq shall вставить их с пустым блоком и не перезаписывать файл.
4. The kq shall в сгенерированном блоке вывести: статистику задач (всего, по статусам), Kanban-доску (todo/in_progress/review/done), ссылки на docs.
5. When передан `--no-readme` в `kq push`, the kq shall пропустить генерацию README.

### Requirement 11: История и diff
**Objective:** As a разработчик, I want просматривать историю изменений, so что понимать, что и когда менялось.

#### Acceptance Criteria
1. When пользователь выполняет `kq log`, the kq shall вывести историю Git-коммитов (hash, дата, сообщение, автор).
2. When пользователь выполняет `kq diff [--from COMMIT]`, the kq shall показать diff изменений.
3. When пользователь выполняет `kq blame <FILE>`, the kq shall показать построчную историю файла.
4. The kq shall поддерживать фильтрацию по пути: `kq log --path docs/`.

### Requirement 12: Multi-repo watcher
**Objective:** As a разработчик, I want watcher следить за knowledge-репозиторием и проектными репозиториями, so что все изменения под версионным контролем.

#### Acceptance Criteria
1. Where в `.kq/knowledge.toml` указан список repo, the kq shall запустить watcher для каждого репозитория из конфига.
2. The kq shall игнорировать те же паттерны (.git, node_modules, target, .obsidian) для всех отслеживаемых репозиториев.
3. When debounce-таймер сработал, the kq shall выполнить авто-коммит независимо для каждого репозитория.

### Requirement 13: LLM-интеграция (kq ask)
**Objective:** As a разработчик, I want задавать вопросы AI из CLI, so что получать ответы с контекстом репозитория.

#### Acceptance Criteria
1. When пользователь выполняет `kq ask <вопрос>`, the kq shall собрать контекст из релевантных документов (через поиск) и отправить в LLM-провайдер.
2. The kq shall поддерживать провайдеры: Ollama (локально), OpenAI API, Anthropic API — через конфиг `.kq/knowledge.toml`.
3. When LLM-провайдер не настроен в конфиге, the kq shall вывести ошибку с инструкцией по настройке.
4. The kq shall выводить ответ в stdout в streaming-режиме (по мере генерации).
5. When передан `--insert <PATH>`, the kq shall вставить ответ в указанный файл.

### Requirement 14: CLI-интерфейс и polish
**Objective:** As a пользователь, I want интуитивный CLI с подсказками и справкой, so что не читать документацию для базовых операций.

#### Acceptance Criteria
1. When пользователь выполняет `kq --help`, the kq shall вывести список всех команд с кратким описанием.
2. When пользователь выполняет `kq <command> --help`, the kq shall вывести подробное описание команды, флаги и примеры.
3. The kq shall выводить цветной вывод в TTY (структурированный, с подсветкой).
4. The kq shall завершаться с exit code 0 при успехе и ненулевым кодом при ошибке.

### Requirement 15: Управление документацией через шаблоны
**Objective:** As a пользователь, I want создавать структурированные документы по стандарту команды через CLI, so что документация едина по структуре и не превращается в хаос.

#### Acceptance Criteria
1. When пользователь выполняет `kq init --with-docs`, the kq shall создать структуру `docs/` по стандарту.
2. When пользователь выполняет `kq doc new bft <title>`, the kq shall создать документ бизнес-функциональных требований по шаблону.
3. When пользователь выполняет `kq doc new brd <title>`, the kq shall создать Business Requirements Document.
4. When пользователь выполняет `kq doc new frd <title>`, the kq shall создать Functional Requirements Document.
5. When пользователь выполняет `kq doc new nfr <title>`, the kq shall создать Non-Functional Requirements.
6. When пользователь выполняет `kq doc new adr <title>`, the kq shall создать Architecture Decision Record со статусом Draft.
7. When пользователь выполняет `kq doc new rfc <title>`, the kq shall создать RFC-документ.
8. When пользователь выполняет `kq doc new tz <title>`, the kq shall создать Техническое Задание.
9. When пользователь выполняет `kq doc new idea <title>`, the kq shall создать документ идеи.
10. When пользователь выполняет `kq doc list`, the kq shall вывести все документы в `docs/` сгруппированные по категориям.
11. When пользователь выполняет `kq doc template --list`, the kq shall показать доступные шаблоны документов.
12. The kq shall хранить шаблоны в `.kq/templates/` и позволять пользовательскую кастомизацию.
13. The kq shall авто-нумеровать документы (ADR-042, TZ-017, BFT-001) на основе существующих файлов.
14. The kq shall заполнять frontmatter (статус, дата, автор) при создании документа.

### Requirement 16: TypeSpec — спецификации объектов данных
**Objective:** As a разработчик, I want описывать объекты данных на языке TypeSpec (.tsp), so что типы и структуры данных имеют единый источник истины.

#### Acceptance Criteria
1. When пользователь выполняет `kq typespec new <ObjectName>`, the kq shall создать `.tsp`-файл в `TypeSpec/` с моделью.
2. When пользователь выполняет `kq typespec list`, the kq shall вывести все модели из `.tsp`-файлов с их `@doc`-ссылками.
3. The kq shall в шапке `.tsp`-файла добавлять комментарии `// @doc TZ-NNN`.
4. The kq shall поддерживать базовые TypeSpec-конструкции: `model`, `enum`, `namespace`, `@doc()`.
5. The kq shall поддерживать структуру: один `.tsp`-файл = одна модель, плюс `main.tsp` для импортов.

### Requirement 17: Screen designs и user flows
**Objective:** As a дизайнер/PM, I want описывать экраны и user flows, so что каждый экран подвязан к ТЗ и спецификации объектов.

#### Acceptance Criteria
1. When пользователь выполняет `kq screen <title>`, the kq shall создать `docs/04-technical-design/screen-NNN-title.md`.
2. When пользователь выполняет `kq userflow <title>`, the kq shall создать `docs/04-technical-design/userflow-NNN-title.md`.
3. The kq shall в шаблонах screen и userflow предусмотреть секцию `@doc` для связей.

### Requirement 19: Editor plugins (Obsidian / VS Code / Zed) — IDEA
**Objective:** As a пользователь, I want работать с задачами и Kanban-доской визуально через редактор, not только через CLI.

**Статус:** IDEA — не реализовано, требуется дизайн.

#### Концепция
Плагины подключаются к kq **через CLI**, не напрямую к файловой системе:

```bash
kq task list --format json          # вернуть JSON для рендеринга Kanban
kq task events TASK-142 --format json  # все события задачи
kq task move TASK-142 review        # drag-and-drop = kq task move
```

#### Почему через CLI
1. Единый источник истины — вся логика в kq, не дублируется в плагине
2. Система типов — все валидации на стороне kq
3. Не нужно знать про `.kq/events/` — плагин вызывает `kq task list --json`
4. Плагин — тонкая прослойка: рендер + drag-and-drop → `kq task move`

#### Целевые редакторы

| Редактор | Как подключается | Что даёт |
|----------|-----------------|----------|
| **Obsidian** | Community plugin (TypeScript, `child_process.exec("kq task list --json")`) | Kanban view, drag-and-drop, @user автокомплит |
| **VS Code** | Extension (`vscode.tasks.executeTask` или shell exec) | Kanban в боковой панели |
| **Zed** | Extension (`command("kq task list --json")`) | Preview + перетаскивание |

#### JSON-формат (для плагинов)
```json
{
  "tasks": [
    {
      "id": "TASK-001",
      "title": "Implement auth",
      "status": "in_progress",
      "assignee": "alex",
      "priority": "P2",
      "created": "2026-07-09",
      "updated": "2026-07-09"
    }
  ],
  "users": {
    "alex": { "name": "Alex Smith" },
    "maria": { "name": "Maria Ivanova" }
  }
}
```

#### Открытые вопросы (к дизайну)
- Нужен ли daemon/watch mode для обновления в реальном времени?
- `kq task events TASK-NNN --json` — все события задачи для отображения истории?
- Autocomplete @user в Obsidian — через `kq user list --json`?
- Как Obsidian запускает `kq` — через системный PATH или нужен `kq serve` (локальный HTTP)?
#### Опция: kq:// protocol handler

Идея: вставлять в .md файлы ссылки вида
```markdown
[Take task](kq://task/assign/TASK-142/alex)
[Move to Review](kq://task/move/TASK-142/review)
```

При клике вызывается `kq task <command>` через зарегистрированный системный протокол.

##### Регистрация схемы
| Платформа | Механизм | Статус |
|-----------|----------|--------|
| **macOS 26+** | `.app` bundle + LaunchServices + **codesign Developer ID** | ⚠️ Требует Developer ID сертификата (платный аккаунт) |
| **macOS (старые)** | `kq install-protocol` → Swift handler + ad-hoc codesign | ✅ Работает |
| **Windows** | `kq install-protocol` → запись в `HKEY_CLASSES_ROOT\kq\shell\open\command` | ✅ |
| **Linux** | `kq install-protocol` → `.desktop` файл через xdg-mime | ✅ |

##### macOS 26: обязательно codesign

Apple заблокировала URL scheme handlers для unsigned `.app`. Ad-hoc подпись (`codesign --sign -`)
недостаточна — требуется полноценный **Developer ID Application** сертификат.

Без подписи `open kq://...` завершается с кодом 0, но процесс handler'а не запускается.

Если есть Developer аккаунт:
```bash
codesign --force --deep --sign "Developer ID Application: Your Name" ~/Applications/kq-handler.app
```

##### Проблема: пользователь в ступоре

При клике на `kq://` в Obsidian/VS Code:
- macOS: `open kq://...` завершается без ошибки — **ноль обратной связи**
- Windows: может открыться пустое окно cmd и сразу закрыться
- Пользователь не понимает, сработало или нет

**Варианты решения:**
1. **OS-нотификация** — `kq` после выполнения показывает macOS Notification Center / Windows Toast
2. **Только через плагин** — плагин редактора вызывает `kq handle-url` напрямую через `child_process.exec()`, без системного протокола

##### Вывод: приоритет — плагин редактора

`kq://` протокол работает без проблем на Windows/Linux. На macOS 26+ только с Developer ID.
Но основная аудитория работает через редакторы (Obsidian, VS Code), где плагин вызывает
`kq handle-url` напрямую, минуя системный протокол. Это надёжнее и даёт обратную связь.

**Текущее решение:** `kq handle-url "kq://task/move/TASK-001/review"` — работает из
терминала на всех платформах. Протокол `kq://` — опциональный бонус для Windows/Linux
пользователей.

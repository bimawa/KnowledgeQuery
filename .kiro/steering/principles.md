# Project Principles: kq

> Status: Active
> Last Updated: 2026-07-09

## Purpose

Эти принципы направляют все фазы SDD — от требований до реализации и ревью.

Если принцип конфликтует с утверждённым SDD-артефактом — остановись и спроси, 
что должно иметь приоритет.

## How to Use These Principles

- `MUST` — нерушимо, если только пользователь явно не изменил
- `SHOULD` — сильный дефолт; исключения требуют обоснования
- `MAY` — разрешено, но не требуется

## Principles

### P-001: Single Binary

**Level:** MUST  
**Rule:** Всё, что нужно пользователю — один статический бинарник. 
Никаких Python, Node, Docker, внешних баз данных.  
**Reason:** Установка = скачал бинарник → запустил. Zero friction.  
**Applies to:** SPEC, TASKS, EXEC, REVIEW

### P-002: No Server / No Web UI

**Level:** MUST  
**Rule:** Никакого веб-сервера, REST API, Web UI. Chistый CLI + background daemon.  
Obsidian (или любой markdown-редактор) — интерфейс для чтения и редактирования.  
**Reason:** kq не заменяет Obsidian. kq решает то, чего нет в Obsidian: sync, 
поиск, AI, таски.  
**Applies to:** PRD, SPEC, TASKS, EXEC, REVIEW

### P-003: Markdown is the Source of Truth

**Level:** MUST  
**Rule:** Все пользовательские данные — .md файлы в Git. Никаких скрытых БД, 
проприетарных форматов, бинарных хранилищ.  
**Reason:** Любой участник работает в удобном редакторе. Git diff читаемый.  
Никакого vendor lock.  
**Applies to:** PRD, SPEC, TASKS, EXEC, REVIEW

### P-004: Offline-First Search

**Level:** MUST  
**Rule:** Векторный поиск работает полностью офлайн. Модель эмбеддингов — 
локально (CPU, Candle). FTS — всегда доступен.  
**Reason:** Пользователь не должен платить за API или иметь интернет для поиска 
по своей базе знаний.  
**Applies to:** SPEC, TASKS, EXEC, REVIEW

### P-005: Auto-Commit, Manual Push

**Level:** SHOULD  
**Rule:** Watcher автоматически коммитит изменения (git add → git commit). 
Push — только по команде `kq push`.  
**Reason:** Пользователь контролирует, что и когда отправляется в remote.  
**Applies to:** PRD, SPEC, TASKS, EXEC

### P-006: README is Auto-Generated

**Level:** SHOULD  
**Rule:** kq обновляет README проекта после каждого push. Ручные правки 
сохраняются — kq пишет только между маркерами `<!-- kq:start -->`.  
**Reason:** README всегда актуален. Никто не забывает обновить статус задач.  
**Applies to:** PRD, SPEC, TASKS, EXEC

### P-007: Minimal Surprise

**Level:** SHOULD  
**Rule:** kq не делает ничего, что пользователь не просил. Watcher коммитит 
локально — не пушит. kq push спрашивает при конфликте.  
**Reason:** Доверие пользователя. kq — инструмент, а не автопилот.  
**Applies to:** SPEC, TASKS, EXEC, REVIEW

### P-008: Trace Graph is a Cache

**Level:** MUST
**Rule:** SQLite-граф трассировки — это кеш. Источник истины — .md файлы с Front Matter. При конфликте графа и .md побеждает .md. Пользователь может удалить `.kq/` и пересобрать граф.
**Reason:** Никакого lock-in. Пользователь всегда владеет данными.
**Applies to:** SPEC, TASKS, EXEC

### P-009: Cross-Repo Scanning — Pull, Not Push

**Level:** SHOULD
**Rule:** kq сканирует внешние репозитории через локальную файловую систему (предполагается clone), не через API. kq НЕ пушит изменения в code-repos.
**Reason:** Безопасность и изоляция. kq не заходит в чужой CI/CD.
**Applies to:** SPEC, TASKS, EXEC

### P-010: Code Anchors — Convention, Not Enforced

**Level:** SHOULD
**Rule:** Аннотации `@doc-anchor` и `@see` в исходниках — опциональный стандарт. kq не ломает сборку при их отсутствии, только предупреждает в `kq check`.
**Reason:** Адаптация gradual. Насилие над командой не работает.
**Applies to:** PRD, SPEC, TASKS

## Decision Rules

1. Safety, privacy, and offline requirements take priority.
2. Simpler solutions preferred unless they fail an approved requirement.
3. User control (manual push) > automation (auto-push).
4. If trade-off unclear — ask the user.

## Review Expectations

- [ ] All MUST principles are respected
- [ ] SHOULD exceptions are documented with reasons
- [ ] Implementation does not add external dependencies beyond approved stack

## Change Policy

- Changes to MUST principles require explicit user approval
- Changed principles do not silently rewrite approved SDD artifacts

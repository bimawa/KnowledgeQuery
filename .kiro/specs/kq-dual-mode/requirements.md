# Requirements: Dual Mode — Dev Context & Doc Context

## Introduction

kq работает в двух контекстах. Один бинарник, два режима, один интерфейс.

**Dev mode:** запускается в репозитории разработчика. Сканирует код на `@doc-anchor`, сверяется с документацией, сообщает об орфанах.

**Doc mode:** запускается в knowledge-репозитории. Управляет документами, TypeSpec, графом трассировки, проверяет целостность.

**CI mode:** doc mode, запущенный в CI/CD при пуше в knowledge-репозиторий. Автоматически перестраивает граф и проверяет орфаны.

## Requirements

### REQ-001: Mode Auto-Detection
kq должен автоматически определять режим работы по контексту:
- Если в корне есть `docs/` + `TypeSpec/` → doc mode
- Если есть `.kq/` с указанием `knowledge.repo` → dev mode
- Если env `CI=true` → doc mode + CI mode

### REQ-002: Explicit Mode Selection
Пользователь должен иметь возможность явно указать режим:
- `kq --dev <command>` / `kq dev <command>`
- `kq --doc <command>` / `kq doc <command>`

### REQ-003: Dev Mode — Scan & Notify
В dev mode kq сканирует код на `@doc-anchor`, сверяется с documentation-репозиторием, сообщает об орфанах. Не имеет права писать в docs-репозиторий.

### REQ-004: Doc Mode — Full Management
В doc mode kq имеет полный доступ ко всем функциям: управление документами, TypeSpec, trace graph, scan внешних проектов.

### REQ-005: CI Mode — Auto-Trace
При пуше в knowledge-репозиторий CI дёргает `kq check traceability --deep`. Если есть орфаны или stale — CI падает с ошибкой (или warning, опционально).

### REQ-006: Knowledge Repo Reference
Dev mode должен знать, где находится knowledge-репозиторий (через config или env).

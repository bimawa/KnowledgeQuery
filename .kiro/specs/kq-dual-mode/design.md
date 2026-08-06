# Design: Dual Mode — Dev Context & Doc Context

> **Spec:** `kq-dual-mode`
> **Language:** ru
> **Status:** Draft

## Overview

Один бинарник `kq`. Два режима. Общий trace graph. Два направления сверки.

```
┌─────────────────────────┐      ┌──────────────────────────┐
│     DEV MODE            │      │     DOC MODE             │
│                         │      │                          │
│  Репозиторий            │      │  Knowledge repo          │
│  разработчика           │      │  docs/ + TypeSpec/       │
│                         │      │                          │
│  @doc-anchor ←─ код     │      │  docs ──> TOC            │
│  @see docs://..         │      │  TypeSpec ──> контракт   │
│                         │      │  trace graph ──> цепь    │
│  знает где docs-repo    │      │  знает проекты ([[..]])  │
│  (config/repo)          │      │                          │
│                         │      │  CI: на push ──> check   │
│  НЕ пишет в docs-repo   │      │                          │
└──────────┬──────────────┘      └─────────────┬────────────┘
           │                                  │
           │         ┌──────────────┐         │
           └─────────► trace_graph  ◄─────────┘
                     │  (SQLite)    │
                     │  орфаны      │
                     │  stale       │
                     │  covered     │
                     └──────────────┘
```

## Mode Detection

### Auto (по умолчанию)

```rust
pub enum KqMode {
    Dev,  // запущен в проекте разработчика
    Doc,  // запущен в knowledge-репозитории
}

pub fn detect_mode() -> KqMode {
    // 1. Явный флаг --dev/--doc
    // 2. Есть docs/ + TypeSpec/? → doc mode
    if Path::new("docs").exists() && Path::new("TypeSpec").exists() {
        return KqMode::Doc;
    }
    // 3. Есть .kq/knowledge.toml с knowledge.repo? → dev mode
    if let Ok(config) = load_dev_config() {
        return KqMode::Dev;
    }
    // 4. fallback: dev mode (без знания docs-repo — limited)
    KqMode::Dev
}
```

### Явный выбор

```bash
kq --dev scan              # явно dev mode
kq --doc check trace       # явно doc mode
```

## Config: Dev Mode

В репозитории разработчика:

```toml
# .kq/knowledge.toml
mode = "dev"

[knowledge]
# Как найти документацию
repo = "git@github.com:org/knowledge.git"
# или локально
# path = "../knowledge"

[scan]
auto = true                 # сканировать при kq watch?
patterns = ["**/*.swift", "**/*.kt", "**/*.go"]
```

## Config: Doc Mode (уже существует)

```toml
# knowledge.toml (текущий)
mode = "doc"
knowledge_path = "."

[[projects]]
path = "../mobile-app"
label = "iOS App"
```

## Что меняется в CLI

### Dev mode — доступные команды

```bash
kq scan                     # сканировать код на @doc-anchor
kq check orphans            # орфаны в коде
kq check notify             # stale-уведомления
kq check trace              # проверка трассировки (read-only)
kq watch                    # watcher + auto-scan
```

**Dev mode НЕ МОЖЕТ:**
- `kq doc new` — создавать документы
- `kq typespec new` — создавать TypeSpec
- `kq readme` — обновлять README docs-репозитория

### Doc mode — доступны все команды

```bash
# Текущие + новые:
kq doc new ...              # создавать документы
kq typespec new ...         # создавать TypeSpec
kq check traceability --deep  # полная трассировка
kq check scan               # сканировать внешние проекты
kq readme                   # обновлять README
kq check notify             # stale + уведомления
```

## CI Integration

### CI mode (на push в knowledge repo)

```yaml
# .github/workflows/ci.yml (дополнение)
- name: Trace check
  run: kq check traceability --deep
  env:
    KQ_MODE: doc
  # Если есть орфаны → warning, не блокируем merge
  # CI=true env автоматически включает ci mode
```

```bash
# Поведение в CI mode:
# - Всегда doc mode (даже если docs/ нет — error)
# - При орфанах: exit code 0 (warning), пишет в stderr
# - При stale: exit code 0 (warning)
# - Можно форсировать fail: kq check trace --deep --fail-on-orphan
```

## Режимы и права

| Операция | Dev | Doc | CI |
|----------|-----|-----|----|
| Scan code | ✅ | ✅ | ❌ |
| Check orphans | ✅ | ✅ | ✅ |
| Doc new/edit | ❌ | ✅ | ❌ |
| TypeSpec new | ❌ | ✅ | ❌ |
| Trace graph rebuild | ✅ (read) | ✅ | ✅ |
| Notify stale | ✅ | ✅ | ✅ |
| README gen | ❌ | ✅ | ❌ |
| Watcher | ✅ | ✅ | ❌ |

## File Changes

| Файл | Изменение |
|------|-----------|
| `kq-cli/src/main.rs` | + `--dev`/`--doc` флаги, mode detection при старте |
| `kq-config/src/lib.rs` | + `KqMode` enum, + `mode` в `KnowledgeConfig` |
| `kq-core/src/lib.rs` | + `mode()`, `is_dev()`, `is_doc()` функции |
| `kq-core/src/check.rs` | Gate: doc-only команды проверяют режим |
| `.github/workflows/ci.yml` | + trace check step |

## Out of Scope

- Web UI / Dashboard (будет отдельно)
- Slack/email интеграция (отдельно)
- Авто-клонирование docs-репозитория в dev mode (пока ручной clone)

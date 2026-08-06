# Implementation Tasks: Dual Mode — Dev Context & Doc Context

## 1. Mode Detection + KqMode enum (kq-config)

**Effort:** M  
**Files:** `kq-config/src/lib.rs`, `kq-core/src/lib.rs`

Добавить `KqMode` enum, функцию `detect_mode()`, экспорт из kq-core.

**Acceptance:**
- `KqMode::Dev` | `KqMode::Doc` с Display
- `detect_mode(path)` проверяет: есть docs/ + TypeSpec/? → Doc
- `detect_mode()` без аргумента — по CWD
- `kq_core::mode()` — глобальный доступ к текущему режиму
- Если docs/ + TypeSpec/ есть, но нет knowledge.toml → всё равно Doc (совместимость)

## 2. Config extension (kq-config)

**Effort:** S  
**Files:** `kq-config/src/lib.rs`

Добавить `mode: KqMode` в `KnowledgeConfig`.

**Acceptance:**
- `knowledge.toml` может содержать `mode = "dev"` или `mode = "doc"`
- Если поля нет — авто-детект
- Если есть — override

## 3. CLI --dev/--doc flags + command gating (kq-cli + kq-core)

**Effort:** M  
**Files:** `kq-cli/src/main.rs`, `kq-core/src/lib.rs`

Флаги `--dev`/`--doc` на верхнем уровне. Gating для doc-only команд.

**Acceptance:**
- `kq --dev ...` / `kq --doc ...` устанавливают режим в глобальное состояние
- `--dev` запускает авто-детект или использует явное указание
- `kq doc new ...` (doc-only) в dev mode → ошибка "This command requires doc mode"
- `kq scan` (доступно в обоих режимах) — работает всегда
- Dev mode команды: scan, check orphans/notify, watch
- Doc mode команды: всё + doc new, typespec new, readme

## 4. CI mode auto-detection

**Effort:** S  
**Files:** `kq-cli/src/main.rs`, `kq-core/src/lib.rs`

При `CI=true` env — включение ci mode.

**Acceptance:**
- `CI=true kq check traceability` — работает в doc mode
- При орфанах — exit code 0, warning в stderr
- exit code 1 только при реальной ошибке (не орфан)

## 5. Build and test all

- `cargo test --workspace` — все тесты проходят
- Проверка mode detection в unit-тестах

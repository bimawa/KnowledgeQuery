# Requirements: Trace Daemon Mode

> **Spec:** `trace-daemon`
> **Language:** ru
> **Status:** Draft

## Introduction

Добавить флаг `--trace` в `kq watch`, который включает фоновую трассировку:
при каждом debounce-цикле watcher перестраивает граф, сканирует проекты
и выводит orphan/stale предупреждения.

**Acceptance:**
1. `kq watch --trace` запускает обычный watcher + на каждом цикле вызывает `rebuild_trace_graph()` и `scan_projects()`
2. При обнаружении orphan anchors или stale links — вывод в stderr
3. `kq watch` без `--trace` работает как раньше

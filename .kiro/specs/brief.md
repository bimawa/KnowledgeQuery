# Discovery Brief: Cross-Repo Traceability & Deep Coverage

## Trigger
Обсуждение архитектуры трассировки зависимостей между документацией и кодом, вдохновленное OpenFastTrace и StrictDoc. Решение: имплементировать глубокую трассировку в kq.

## Feature Breakdown

### Wave 1: Trace Graph DB & Deep Coverage (P0)
Замена одноразового парсинга `@doc`-ссылок на SQLite-граф связей. Deep coverage — проверка полных цепочек (BFT→ADR→TZ→code), а не только поверхностных ссылок.

**Ключевые изменения:**
- Новая SQLite-таблица `trace_links` с source/target/status/revision
- Парсинг `needs` и `covers` из Front Matter документов
- Алгоритм deep coverage (транзитивное замыкание)
- Обновление `kq check traceability` для работы через граф

### Wave 2a: Code Anchors & Cross-Repos Scanning (P0)
- `@doc-anchor <name>` в исходниках (Swift, Go, Kotlin, Python, Rust)
- `@see docs://<path>` обратные ссылки из кода
- `code_anchors: [...]` в Front Matter документов
- Cross-repo сканер из `.kq/knowledge.toml`
- Матчинг: Anchor Tag → Front Matter → Doc

### Wave 2b: Spec Item Revision Tracking (P1)
- ID с ревизией: `ADR-0002~1`
- Stale-детекция при инкременте ревизии
- Предупреждение: "документ изменился, проверь связи"

### Wave 2c: Traceability Daemon (P1)
- `kq watch --trace` — фоновый re-index графа
- Автоматическое обнаружение сирот и битых ссылок
- Уведомления через stderr/callback

## Action Path
Multi-spec: Wave 1 (base) → Wave 2 (параллельно 2a, 2b, 2c)

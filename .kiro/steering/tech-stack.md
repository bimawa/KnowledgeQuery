# Tech Stack Steering: kqs

## Runtime / Platform

- **Язык:** Rust (edition 2024)
- **Target:** macOS (arm64), Linux (x86_64, aarch64)
- **Сборка:** `cargo` workspace, статическая линковка (musl для Linux)
- **Single binary:** Всё в одном бинарнике, модель эмбеддингов скачивается при первом `kqs search`

## Crates (Workspace)

| Crate | Type | Purpose |
|-------|------|---------|
| `kqs` | bin | CLI entry point, clap-команды |
| `kq-core` | lib | Watcher, search engine, git ops, task manager, README gen |
| `kq-embeddings` | lib | Candle + all-MiniLM-L6-v2, эмбеддинги |
| `kq-llm` | lib | LLM provider abstraction (Ollama/OpenAI/Anthropic) |
| `kq-config` | lib | Парсинг knowledge.toml, пути, провайдеры |

## Key Dependencies

| Dependency | Crate | Purpose |
|------------|-------|---------|
| `clap` v4 | kqs | CLI argument parsing |
| `git2` | kq-core | Git операции (libgit2 bindings) |
| `notify` | kq-core | Файловый watcher |
| `rusqlite` | kq-core | SQLite + FTS5 |
| `sqlite-vec` | kq-core | Векторный поиск в SQLite |
| `candle` + `candle-nn` | kq-embeddings | Pure Rust ML inference |
| `hf-hub` | kq-embeddings | Скачивание модели all-MiniLM-L6-v2 |
| `reqwest` (async) | kq-llm | HTTP к LLM-провайдерам |
| `serde` + `toml` | kq-config | Конфиг (knowledge.toml) |
| `tokio` | kq-core | Async runtime для watcher + LLM |

| `regex` | kq-core | Cross-repo scan: поиск @doc-anchor/@see в исходниках |
| `walkdir` | kq-core | Рекурсивное сканирование внешних репозиториев |
## Verification Commands

- Build: `cargo build --workspace`
- Lint: `cargo clippy --workspace -- -D warnings`
- Test: `cargo test --workspace`
- Format: `cargo fmt --check`

## Constraints

- Никаких внешних сервисов для базового поиска (FTS + векторный)
- LLM — опционально, через конфиг (Ollama/OpenAI/Anthropic)
- Модель эмбеддингов — CPU only (Candle), никакого CUDA/CUDA toolkit
- sqlite-vec — встроенная векторная БД, Qdrant только как опциональная замена

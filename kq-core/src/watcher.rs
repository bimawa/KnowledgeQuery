use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Config, Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use kq_config::KnowledgeConfig;

/// Directories whose contents are always ignored during file watching.
const DEFAULT_IGNORE_DIRS: &[&str] =
    &[".git", ".obsidian", "node_modules", "target", "db.sqlite", "state.db", "model-cache"];

/// A filtered filesystem event produced by the notify watcher.
#[derive(Debug)]
struct FileEvent {
    #[allow(dead_code)]
    paths: Vec<PathBuf>,
    #[allow(dead_code)]
    kind: EventKind,
}

/// Expand a leading `~` to the home directory, leaving other paths unchanged.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix('~')
        && let Some(home) = dirs::home_dir()
    {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return if rest.is_empty() { home } else { home.join(rest) };
    }
    path.to_path_buf()
}

/// Collect all directories that should be watched from the config.
///
/// Returns the knowledge repository path followed by any linked project
/// directories. Only existing directories are included.
fn collect_watch_paths(config: &KnowledgeConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let repo_dir = expand_tilde(&config.knowledge_path);
    if repo_dir.is_dir() {
        paths.push(repo_dir);
    }

    for project in &config.projects {
        let expanded = expand_tilde(&project.path);
        if expanded.is_dir() {
            paths.push(expanded);
        } else {
            eprintln!("[kqs] Warning: project path '{}' is not a directory — skipping", project.path.display());
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Determine whether a filesystem event should be processed further.
///
/// Only `Create`, `Modify`, and `Remove` events are accepted.  `Access`
/// events are discarded.  A path matching any ignored directory component
/// is also discarded.
fn is_relevant_event(event: &Event, ignore_dirs: &[String]) -> bool {
    // Discard Access-only events
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }

    // At least one path must pass the ignore filter
    for path in &event.paths {
        let path_str = path.display().to_string();
        let ignored = ignore_dirs.iter().any(|pat| path_str.contains(pat.as_str()));
        if !ignored {
            return true;
        }
    }

    false
}

/// Handle a single debounce tick for a watched directory.
///
/// 1. Opens the git repository (or reports the error once).
/// 2. Stages all changes and creates an auto-sync commit (with retries).
/// 3. Re-indexes changed files in the knowledge database.
fn handle_debounce_tick(dir: &Path) {
    // 1. Regenerate README from task files
    if crate::state_dir(dir).is_dir()
        && let Err(e) = crate::readme_gen::generate(dir)
    {
        eprintln!("[kqs] README generation skipped: {e:#}");
    }

    let repo = match crate::git::open_repo(dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[kqs] Cannot open git repository at {}: {e:#}", dir.display());
            return;
        }
    };

    match crate::git::auto_commit_with_retry(&repo, dir) {
        Ok(Some(oid)) => {
            eprintln!("[kqs] Committed {oid} (dir: {})", dir.display());

            // Re-index changed files
            match crate::indexer::index_all(dir) {
                Ok(report) => {
                    if report.indexed > 0 {
                        eprintln!("[kqs] Re-indexed: {} files indexed, {} skipped", report.indexed, report.skipped);
                    }
                    if report.failed > 0 {
                        eprintln!("[kqs] Re-index failures: {}", report.failed);
                    }
                }
                Err(e) => {
                    eprintln!("[kqs] Re-index skipped (DB not initialized?): {e:#}");
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("[kqs] Auto-commit failed for {}: {e:#}", dir.display());
        }
    }
}

/// Start the filesystem watcher daemon.
///
/// Watches all directories described by `config`, debounces filesystem
/// events, and on each tick auto-commits changes to git and re-indexes
/// the knowledge database.
///
/// Blocks until SIGINT (Ctrl+C) or SIGTERM is received.
pub async fn start_watch(config: &KnowledgeConfig) -> Result<()> {
    let watch_paths = collect_watch_paths(config);
    if watch_paths.is_empty() {
        anyhow::bail!("No directories to watch.  Check your knowledge.toml configuration.");
    }

    let debounce = Duration::from_secs(std::cmp::max(1, config.watcher.debounce_secs));

    // --- Channel: notify thread → async handler ---
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<FileEvent>();

    // --- Shutdown channel ---
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // --- Signal handling (SIGINT + SIGTERM) ---
    let sig_sender = shutdown_tx;
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut sigint = match signal(SignalKind::interrupt()) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[kqs] Failed to register SIGINT handler: {e}");
                    return;
                }
            };
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[kqs] Failed to register SIGTERM handler: {e}");
                    return;
                }
            };

            tokio::select! {
                _ = sigint.recv() => {
                    eprintln!("[kqs] Received SIGINT — shutting down...");
                    let _ = sig_sender.send(());
                }
                _ = sigterm.recv() => {
                    eprintln!("[kqs] Received SIGTERM — shutting down...");
                    let _ = sig_sender.send(());
                }
            }
        }

        #[cfg(windows)]
        {
            if let Err(e) = tokio::signal::ctrl_c().await {
                eprintln!("[kqs] Failed to register Ctrl+C handler: {e}");
                return;
            }
            eprintln!("[kqs] Received Ctrl+C — shutting down...");
            let _ = sig_sender.send(());
        }
    });

    // --- Build ignore pattern list ---
    let mut ignore_dirs: Vec<String> = DEFAULT_IGNORE_DIRS.iter().map(|s| (*s).to_string()).collect();
    ignore_dirs.extend(config.watcher.ignore_patterns.clone());
    let ignore_dirs = Arc::new(ignore_dirs);

    // --- Spawn the notify watcher in a std thread ---
    let tx = event_tx;
    let paths = watch_paths.clone();
    let ignores = ignore_dirs;

    std::thread::spawn(move || {
        let event_tx = tx;
        let config = Config::default().with_poll_interval(Duration::from_secs(2));

        let mut watcher: PollWatcher = match PollWatcher::new(
            move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res
                    && is_relevant_event(&event, &ignores)
                {
                    let _ = event_tx.send(FileEvent { paths: event.paths, kind: event.kind });
                }
            },
            config,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[kqs] Failed to create filesystem watcher: {e}");
                return;
            }
        };

        for path in &paths {
            if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                eprintln!("[kqs] Failed to watch {}: {e}", path.display());
            } else {
                eprintln!("[kqs] Watching: {}", path.display());
            }
        }

        // Keep the watcher alive until the process exits
        loop {
            std::thread::sleep(Duration::from_secs(u64::MAX));
        }
    });

    // Brief pause so watcher-setup messages print before the "started" line
    tokio::time::sleep(Duration::from_millis(100)).await;

    eprintln!(
        "[kqs] Watcher started. {} director(ies), debounce = {}s. Press Ctrl+C to stop.",
        watch_paths.len(),
        debounce.as_secs()
    );

    // --- Main debounce loop ---
    let poll_interval = Duration::from_millis(250);
    let mut debounce_deadline: Option<tokio::time::Instant> = None;
    let mut last_tick = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                eprintln!("[kqs] Shutting down...");
                break;
            }
            Some(_event) = event_rx.recv() => {
                // Cooldown: ignore events for 2s after a tick to prevent loops
                if tokio::time::Instant::now().duration_since(last_tick) < Duration::from_secs(2) {
                    continue;
                }
                eprintln!(
                    "[kqs] Change detected — resetting debounce timer ({}s)",
                    debounce.as_secs()
                );
                debounce_deadline = Some(tokio::time::Instant::now() + debounce);
            }
            _ = tokio::time::sleep(poll_interval) => {
                if let Some(deadline) = debounce_deadline
                    && tokio::time::Instant::now() >= deadline
                {
                    debounce_deadline = None;
                    last_tick = tokio::time::Instant::now();
                    eprintln!("[kqs] Debounce tick — processing changes");
                    for dir in &watch_paths {
                        handle_debounce_tick(dir);
                    }
                }
            }
        }
    }

    eprintln!("[kqs] Watcher stopped.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::CreateKind;
    use std::fs;

    // ------------------------------------------------------------------
    // collect_watch_paths
    // ------------------------------------------------------------------

    #[test]
    fn test_collect_watch_paths_nonexistent_dir_returns_empty() {
        // Use a path that definitely does not exist.
        let config =
            KnowledgeConfig { knowledge_path: "/this/path/does/not/exist/anywhere".into(), ..Default::default() };
        let paths = collect_watch_paths(&config);
        assert!(paths.is_empty(), "non-existent path should produce no watch dirs");
    }

    #[test]
    fn test_collect_watch_paths_with_temp_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut config = KnowledgeConfig::default();
        config.knowledge_path = tmp.path().to_path_buf();
        let paths = collect_watch_paths(&config);
        assert!(paths.contains(&tmp.path().to_path_buf()));
    }

    #[test]
    fn test_collect_watch_paths_deduplicates() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut config = KnowledgeConfig::default();
        config.knowledge_path = tmp.path().to_path_buf();
        config.projects.push(kq_config::ProjectConfig {
            path: tmp.path().to_path_buf(),
            label: None,
            scan_patterns: vec![],
        });
        let paths = collect_watch_paths(&config);
        assert_eq!(paths.len(), 1);
    }

    // ------------------------------------------------------------------
    // expand_tilde
    // ------------------------------------------------------------------

    #[test]
    fn test_expand_tilde_no_tilde() {
        let p = Path::new("/home/user/foo");
        assert_eq!(expand_tilde(p), p);
    }

    #[test]
    fn test_expand_tilde_only_tilde() {
        let result = expand_tilde(Path::new("~"));
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        }
    }

    #[test]
    fn test_expand_tilde_with_suffix() {
        let result = expand_tilde(Path::new("~/foo/bar"));
        if let Some(home) = dirs::home_dir() {
            let expected = home.join("foo/bar");
            assert_eq!(result, expected);
        }
    }

    // ------------------------------------------------------------------
    // is_relevant_event
    // ------------------------------------------------------------------

    #[test]
    fn test_is_relevant_event_access_is_ignored() {
        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Close(notify::event::AccessMode::Write)),
            paths: vec![PathBuf::from("/tmp/foo.md")],
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(!is_relevant_event(&event, &[]));
    }

    #[test]
    fn test_is_relevant_event_create_is_accepted() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/tmp/foo.md")],
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(is_relevant_event(&event, &[]));
    }

    #[test]
    fn test_is_relevant_event_ignored_path() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/repo/.git/HEAD")],
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(!is_relevant_event(&event, &[".git".to_string()]));
    }

    #[test]
    fn test_is_relevant_event_partial_ignore_keeps_others() {
        // One path is ignored, but another is not → still relevant
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/repo/.git/HEAD"), PathBuf::from("/repo/docs/foo.md")],
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(is_relevant_event(&event, &[".git".to_string()]));
    }

    #[test]
    fn test_is_relevant_event_all_ignored() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/repo/node_modules/pkg/index.js"), PathBuf::from("/repo/target/debug/kqs")],
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(!is_relevant_event(&event, &["node_modules".to_string(), "target".to_string()]));
    }

    // ------------------------------------------------------------------
    // handle_debounce_tick (integration — uses actual git)
    // ------------------------------------------------------------------

    #[test]
    fn test_handle_debounce_tick_non_repo_dir() {
        // Should not panic — just log an error
        let tmp = tempfile::TempDir::new().unwrap();
        handle_debounce_tick(tmp.path()); // no panic = pass
    }

    #[test]
    fn test_handle_debounce_tick_repo_with_changes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();

        // Create an initial commit so HEAD exists
        let sig = git2::Signature::now("test", "test@test").unwrap();
        let mut index = repo.index().unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Write a new file
        fs::write(tmp.path().join("test.md"), "# Watcher test\n").unwrap();

        // Run the handler — should succeed without panic
        handle_debounce_tick(tmp.path());

        // Verify a commit was made
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let msg = head.message().unwrap();
        assert!(msg.starts_with("docs: auto-sync ["));
    }
}

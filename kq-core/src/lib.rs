pub mod check;
pub mod code_anchor;
pub mod conflict;
pub mod db;
pub mod docs;
pub mod git;
pub mod history;
pub mod indexer;
pub mod init;
pub mod llm_help;
pub mod push;
pub mod readme_gen;
pub mod search;
pub mod task;
pub mod typespec;
pub mod vector;
pub mod watcher;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Internal state directory name inside a knowledge repo.
pub const STATE_DIR: &str = ".kqs";

/// Resolve the state directory (`.kqs`) for a knowledge repo.
///
/// On first use, migrates a legacy `.kq` directory in place so existing
/// repositories keep their database, events and templates.
pub fn state_dir(repo_path: &Path) -> PathBuf {
    let new = repo_path.join(STATE_DIR);
    let old = repo_path.join(".kq");
    if old.is_dir() && !new.exists() {
        match std::fs::rename(&old, &new) {
            Ok(()) => eprintln!("[kqs] Migrated legacy state directory .kq -> {STATE_DIR}"),
            Err(e) => eprintln!("[kqs] Failed to migrate .kq -> {STATE_DIR}: {e}"),
        }
    }
    new
}

/// Operating mode for kqs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KqsMode {
    /// Running in a developer project — limited read-only ops on docs.
    Dev,
    /// Running in the knowledge repo — full access to all commands.
    Doc,
}

impl std::fmt::Display for KqsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KqsMode::Dev => write!(f, "dev"),
            KqsMode::Doc => write!(f, "doc"),
        }
    }
}

/// Global mode, set once at startup.
static CURRENT_MODE: OnceLock<KqsMode> = OnceLock::new();

/// Set the global mode explicitly (from CLI --dev/--doc).
pub fn set_mode(mode: KqsMode) {
    let _ = CURRENT_MODE.set(mode);
}

/// Get the current mode.
pub fn mode() -> KqsMode {
    *CURRENT_MODE.get().expect("KqsMode not initialized")
}

/// Detect mode from environment and filesystem.
pub fn detect_mode(path: &Path) -> KqsMode {
    if let Ok(val) = std::env::var("KQS_MODE") {
        match val.as_str() {
            "dev" => return KqsMode::Dev,
            "doc" => return KqsMode::Doc,
            _ => {}
        }
    }
    if std::env::var("CI").map(|v| v == "true").unwrap_or(false) {
        return KqsMode::Doc;
    }
    if path.join("docs").exists() && path.join("TypeSpec").exists() {
        return KqsMode::Doc;
    }
    KqsMode::Dev
}

pub fn is_dev() -> bool {
    mode() == KqsMode::Dev
}
pub fn is_doc() -> bool {
    mode() == KqsMode::Doc
}

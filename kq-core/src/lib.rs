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

use std::path::Path;
use std::sync::OnceLock;

/// Operating mode for kq.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KqMode {
    /// Running in a developer project — limited read-only ops on docs.
    Dev,
    /// Running in the knowledge repo — full access to all commands.
    Doc,
}

impl std::fmt::Display for KqMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KqMode::Dev => write!(f, "dev"),
            KqMode::Doc => write!(f, "doc"),
        }
    }
}

/// Global mode, set once at startup.
static CURRENT_MODE: OnceLock<KqMode> = OnceLock::new();

/// Set the global mode explicitly (from CLI --dev/--doc).
pub fn set_mode(mode: KqMode) {
    let _ = CURRENT_MODE.set(mode);
}

/// Get the current mode.
pub fn mode() -> KqMode {
    *CURRENT_MODE.get().expect("KqMode not initialized")
}

/// Detect mode from environment and filesystem.
pub fn detect_mode(path: &Path) -> KqMode {
    if let Ok(val) = std::env::var("KQ_MODE") {
        match val.as_str() {
            "dev" => return KqMode::Dev,
            "doc" => return KqMode::Doc,
            _ => {}
        }
    }
    if std::env::var("CI").map(|v| v == "true").unwrap_or(false) {
        return KqMode::Doc;
    }
    if path.join("docs").exists() && path.join("TypeSpec").exists() {
        return KqMode::Doc;
    }
    KqMode::Dev
}

pub fn is_dev() -> bool { mode() == KqMode::Dev }
pub fn is_doc() -> bool { mode() == KqMode::Doc }

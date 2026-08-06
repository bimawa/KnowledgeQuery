use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

/// Returns the default knowledge path — the current working directory.
pub fn default_knowledge_path() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve the knowledge repository path.
///
/// Resolution order:
/// 1. If `custom` is Some and non-empty, use it directly (with tilde expansion).
/// 2. Walk up from current working directory looking for `knowledge.toml`.
///    If found, deserialize it and use the `knowledge_path` field.
/// 3. Fall back to `~/.knowledge/` (with tilde expansion).
///
/// The `--repo` CLI flag should be passed as `custom` to override auto-detection.
pub fn repo_path(custom: Option<&str>) -> Result<PathBuf> {
    if let Some(custom_path) = custom
        && !custom_path.is_empty()
    {
        return Ok(expand_tilde(Path::new(custom_path)));
    }

    // Walk up from cwd looking for knowledge.toml
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd.as_path());
        while let Some(d) = dir {
            let config_path = d.join("knowledge.toml");
            if config_path.exists() {
                // Try to deserialize and use knowledge_path field
                if let Ok(config) = KnowledgeConfig::load(&config_path) {
                    let expanded = expand_tilde(&config.knowledge_path);
                    return Ok(expanded);
                }
                // Config exists but is unreadable; still use its parent dir as hint
                return Ok(d.to_path_buf());
            }
            dir = d.parent();
        }
    }

    // Fallback: check ~/.kq/current-repo marker
    if let Some(home) = home_dir() {
        let marker = home.join(".kq/current-repo");
        if marker.exists() {
            let content = std::fs::read_to_string(&marker).unwrap_or_default();
            let path = content.trim().to_string();
            if !path.is_empty() {
                let expanded = expand_tilde(Path::new(&path));
                if expanded.join("knowledge.toml").exists() {
                    return Ok(expanded);
                }
            }
        }
    }

    // Fall back to default
    Ok(expand_tilde(&default_knowledge_path()))
}

/// Expand `~` at the start of a path to the user's home directory.
/// If the path doesn't start with `~`, return it unchanged.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix('~')
        && let Some(home) = home_dir()
        && (rest.is_empty() || rest.starts_with('/'))
    {
        let mut buf = home;
        if !rest.is_empty() {
            // Skip the leading '/'
            buf.push(&rest[1..]);
        }
        return buf;
    }
    path.to_path_buf()
}

/// Get the home directory from environment variables.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Top-level configuration for a knowledge repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    /// Path to the knowledge repository (default: ~/.knowledge/).
    pub knowledge_path: PathBuf,
    /// Linked project repositories.
    pub projects: Vec<ProjectConfig>,
    /// File watcher settings.
    pub watcher: WatcherConfig,
    /// Optional LLM configuration.
    pub llm: Option<LlmConfig>,
    /// Search / embedding settings.
    pub search: SearchConfig,
    /// Optional remote URL (e.g. `origin` remote for the git repo).
    pub remote: Option<String>,
    /// Operating mode: "dev", "doc", or empty for auto-detect.
    #[serde(default)]
    pub mode: String,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            knowledge_path: default_knowledge_path(),
            projects: Vec::new(),
            watcher: WatcherConfig::default(),
            llm: None,
            search: SearchConfig::default(),
            remote: None,
            mode: String::new(),
        }
    }
}

impl KnowledgeConfig {
    /// Load a `KnowledgeConfig` from a TOML file at the given path.
    ///
    /// Fields omitted from the file will be filled with their default values
    /// thanks to `#[serde(default)]` on the struct.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let contents = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse TOML config from: {}", path.as_ref().display()))?;
        Ok(config)
    }

    /// Serialize this config to a TOML-formatted string.
    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize config to TOML")
    }
}

/// Default file patterns for code anchor scanning.
pub const DEFAULT_SCAN_PATTERNS: &[&str] = &[
    "**/*.rs",
    "**/*.go",
    "**/*.swift",
    "**/*.kt",
    "**/*.java",
    "**/*.py",
    "**/*.rb",
    "**/*.ts",
    "**/*.js",
    "**/*.tsx",
    "**/*.jsx",
    "**/*.c",
    "**/*.cpp",
    "**/*.h",
    "**/*.hpp",
    "**/*.m",
    "**/*.mm",
    "**/*.sh",
    "**/*.yaml",
    "**/*.yml",
    "**/*.html",
    "**/*.xml",
];

/// Configuration for a linked project repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    /// Filesystem path to the project.
    pub path: PathBuf,
    /// Optional human-readable label for the project.
    pub label: Option<String>,
    /// File glob patterns for code anchor scanning.
    /// Defaults to common source file extensions.
    #[serde(default = "default_scan_patterns")]
    pub scan_patterns: Vec<String>,
}
impl ProjectConfig {
    /// Returns the scan patterns, falling back to defaults if empty.
    pub fn effective_scan_patterns(&self) -> &[String] {
        if !self.scan_patterns.is_empty() { &self.scan_patterns } else { get_default_scan_patterns_str() }
    }
}

/// Static default scan patterns as Strings (lazy-init).
static DEFAULT_SCAN_PATTERNS_STR: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
fn get_default_scan_patterns_str() -> &'static Vec<String> {
    DEFAULT_SCAN_PATTERNS_STR.get_or_init(|| DEFAULT_SCAN_PATTERNS.iter().map(|s| s.to_string()).collect())
}

fn default_scan_patterns() -> Vec<String> {
    DEFAULT_SCAN_PATTERNS.iter().map(|s| s.to_string()).collect()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            label: None,
            scan_patterns: DEFAULT_SCAN_PATTERNS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// File watcher settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherConfig {
    /// Debounce period in seconds before detecting changes (default: 60).
    pub debounce_secs: u64,
    /// Glob patterns to ignore when watching files.
    pub ignore_patterns: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { debounce_secs: 60, ignore_patterns: Vec::new() }
    }
}

/// LLM provider configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name (e.g. "openai", "anthropic", "ollama").
    pub provider: String,
    /// API endpoint URL.
    pub endpoint: String,
    /// Model identifier (e.g. "gpt-4", "claude-3-opus").
    pub model: String,
    /// Optional API key. If omitted, the application may look for an
    /// environment variable or prompt the user.
    pub api_key: Option<String>,
}

/// Search / embedding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Number of tokens per chunk (default: 512).
    pub chunk_size: usize,
    /// Overlap between consecutive chunks (default: 64).
    pub chunk_overlap: usize,
    /// Maximum number of search results to return (default: 10).
    pub max_results: usize,
    /// Minimum relevance score threshold (default: 0.7).
    pub relevance_threshold: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self { chunk_size: 512, chunk_overlap: 64, max_results: 10, relevance_threshold: 0.7 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample valid TOML string for round-trip and parsing tests.
    ///
    /// Root-level keys (`knowledge_path`, `remote`) must appear before
    /// any table header (`[watcher]`, `[llm]`, `[search]`) so the TOML
    /// parser assigns them to the root struct (`KnowledgeConfig`).
    const SAMPLE_TOML: &str = r#"
knowledge_path = "/home/user/.knowledge/"
remote = "https://github.com/user/knowledge.git"

[[projects]]
path = "/home/user/projects/foo"
label = "Foo Project"

[[projects]]
path = "/home/user/projects/bar"

[watcher]
debounce_secs = 60
ignore_patterns = ["node_modules/", "target/"]

[llm]
provider = "openai"
endpoint = "https://api.openai.com/v1"
model = "gpt-4"
api_key = "sk-..."

[search]
chunk_size = 256
chunk_overlap = 32
max_results = 20
relevance_threshold = 0.8
"#;

    #[test]
    fn parse_sample_toml() {
        let config: KnowledgeConfig = toml::from_str(SAMPLE_TOML).unwrap();
        assert_eq!(config.knowledge_path, PathBuf::from("/home/user/.knowledge/"));
        assert_eq!(config.projects.len(), 2);
        assert_eq!(config.projects[0].label.as_deref(), Some("Foo Project"));
        assert_eq!(config.projects[1].label, None);
        assert_eq!(config.watcher.debounce_secs, 60);
        assert_eq!(config.watcher.ignore_patterns, vec!["node_modules/", "target/"]);
        assert!(config.llm.is_some());
        let llm = config.llm.unwrap();
        assert_eq!(llm.provider, "openai");
        assert_eq!(llm.endpoint, "https://api.openai.com/v1");
        assert_eq!(llm.model, "gpt-4");
        assert_eq!(llm.api_key, Some("sk-...".to_string()));
        assert_eq!(config.search.chunk_size, 256);
        assert_eq!(config.search.chunk_overlap, 32);
        assert_eq!(config.search.max_results, 20);
        assert_eq!(config.search.relevance_threshold, 0.8);
        assert_eq!(config.remote, Some("https://github.com/user/knowledge.git".to_string()));
    }

    #[test]
    fn default_config_roundtrip() {
        let default_config = KnowledgeConfig::default();
        let toml_str = default_config.to_toml_string().unwrap();
        let parsed: KnowledgeConfig = toml::from_str(&toml_str).unwrap();
        // Compare field by field since PathBuf equality is what we want
        assert_eq!(default_config.knowledge_path, parsed.knowledge_path);
        assert_eq!(default_config.projects, parsed.projects);
        assert_eq!(default_config.watcher.debounce_secs, parsed.watcher.debounce_secs);
        assert_eq!(default_config.watcher.ignore_patterns, parsed.watcher.ignore_patterns);
        assert_eq!(default_config.llm, parsed.llm);
        assert_eq!(default_config.search.chunk_size, parsed.search.chunk_size);
        assert_eq!(default_config.search.chunk_overlap, parsed.search.chunk_overlap);
        assert_eq!(default_config.search.max_results, parsed.search.max_results);
        // Allow small floating-point comparison for relevance_threshold
        assert!((default_config.search.relevance_threshold - parsed.search.relevance_threshold).abs() < f32::EPSILON);
        assert_eq!(default_config.remote, parsed.remote);
    }

    #[test]
    fn missing_fields_get_defaults() {
        // Only provide a knowledge_path, everything else should be defaulted
        let partial_toml = r#"knowledge_path = "/custom/path""#;
        let config: KnowledgeConfig = toml::from_str(partial_toml).unwrap();
        assert_eq!(config.knowledge_path, PathBuf::from("/custom/path"));
        // Defaults
        assert_eq!(config.watcher.debounce_secs, 60);
        assert!(config.watcher.ignore_patterns.is_empty());
        assert!(config.llm.is_none());
        assert_eq!(config.search.chunk_size, 512);
        assert_eq!(config.search.chunk_overlap, 64);
        assert_eq!(config.search.max_results, 10);
        assert!((config.search.relevance_threshold - 0.7).abs() < f32::EPSILON);
        assert!(config.remote.is_none());
    }

    #[test]
    fn invalid_toml_returns_error() {
        let invalid = "this is not valid toml = {{{";
        let result: Result<KnowledgeConfig> = toml::from_str(invalid).map_err(|e| anyhow::anyhow!("{}", e));
        assert!(result.is_err());
    }

    #[test]
    fn missing_file_returns_error() {
        let result = KnowledgeConfig::load("/tmp/nonexistent_kq_config_12345.toml");
        assert!(result.is_err());
    }

    #[test]
    fn load_and_roundtrip_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_knowledge_config.toml");
        let config = KnowledgeConfig::default();
        let toml_str = config.to_toml_string().unwrap();
        std::fs::write(&path, &toml_str).unwrap();
        let loaded = KnowledgeConfig::load(&path).unwrap();
        assert_eq!(config.knowledge_path, loaded.knowledge_path);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn default_knowledge_path_returns_cwd() {
        let p = default_knowledge_path();
        assert!(p.is_absolute() || p == PathBuf::from("."));
        assert!(!p.to_string_lossy().contains(".knowledge"));
    }

    #[test]
    fn watcher_config_default() {
        let w = WatcherConfig::default();
        assert_eq!(w.debounce_secs, 60);
    }

    #[test]
    fn search_config_default() {
        let s = SearchConfig::default();
        assert_eq!(s.chunk_size, 512);
        assert_eq!(s.chunk_overlap, 64);
        assert_eq!(s.max_results, 10);
        assert!((s.relevance_threshold - 0.7).abs() < f32::EPSILON);
    }
}

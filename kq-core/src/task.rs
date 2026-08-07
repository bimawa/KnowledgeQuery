use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

/// A CRDT event: one operation on a task, stored as a .md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskEvent {
    task: String,
    op: String,
    value: String,
    timestamp: String,
}

/// Computed state from replaying events.
#[derive(Debug, Clone)]
pub struct ComputedState {
    pub status: Status,
    pub assignee: String,
    pub updated: String,
}

impl Default for ComputedState {
    fn default() -> Self {
        Self { status: Status::Todo, assignee: String::new(), updated: String::new() }
    }
}

/// Replay events for a task and compute the current state.
fn replay_events(events_dir: &Path) -> Result<ComputedState> {
    let mut state = ComputedState::default();
    if !events_dir.exists() {
        return Ok(state);
    }

    let mut entries: Vec<_> = fs::read_dir(events_dir)
        .context("Failed to read events directory")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let content = fs::read_to_string(entry.path())
            .with_context(|| format!("Failed to read event {}", entry.path().display()))?;
        let event = parse_event(&content)?;
        match event.op.as_str() {
            "create" => {
                state.updated = event.timestamp.clone();
            }
            "assign" => {
                state.assignee = event.value;
                state.updated = event.timestamp.clone();
            }
            "move" => {
                state.status = event.value.parse().unwrap_or(Status::Todo);
                state.updated = event.timestamp.clone();
            }
            _ => {}
        }
    }

    Ok(state)
}

/// Parse a TaskEvent from a .md file with YAML frontmatter.
fn parse_event(content: &str) -> Result<TaskEvent> {
    let stripped = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| anyhow::anyhow!("Event file missing YAML frontmatter"))?;
    let event: TaskEvent = serde_yaml::from_str(stripped).context("Failed to parse event frontmatter")?;
    Ok(event)
}

/// Write an event file to `.kqs/events/<task_id>/`.
fn write_event(repo_path: &Path, task_id: &str, op: &str, value: &str) -> Result<TaskEvent> {
    let events_dir = crate::state_dir(repo_path).join("events").join(task_id);
    fs::create_dir_all(&events_dir).context("Failed to create events directory")?;

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let filename = format!("{}-{}-{}.md", timestamp, op, value);
    let file_path = events_dir.join(filename);

    let event = TaskEvent {
        task: task_id.to_string(),
        op: op.to_string(),
        value: value.to_string(),
        timestamp: timestamp.clone(),
    };

    let yaml = serde_yaml::to_string(&event).context("Failed to serialize event")?;
    let content = format!("---\n{}---\n\n# {}: {} — {}\n", yaml, task_id, op, value);

    fs::write(&file_path, &content).with_context(|| format!("Failed to write event {}", file_path.display()))?;

    Ok(event)
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

/// Create a user profile file at `users/<username>.md`.
pub fn user_create(username: &str, display_name: Option<&str>) -> Result<String> {
    let repo = kq_config::repo_path(None)?;
    let users_dir = repo.join("users");
    fs::create_dir_all(&users_dir).context("Failed to create users directory")?;

    let file_path = users_dir.join(format!("{}.md", username));
    if file_path.exists() {
        bail!("User '{}' already exists at {}", username, file_path.display());
    }

    let name = display_name.unwrap_or(username);
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let content =
        format!("---\nusername: {}\nname: {}\ncreated: {}\nrole: member\n---\n\n# {}\n\n", username, name, date, name);
    fs::write(&file_path, &content).with_context(|| format!("Failed to write user file {}", file_path.display()))?;

    Ok(username.to_string())
}

/// List all users, returning (username, display_name) pairs.
pub fn user_list() -> Result<Vec<(String, String)>> {
    let repo = kq_config::repo_path(None)?;
    let users_dir = repo.join("users");
    if !users_dir.exists() {
        return Ok(Vec::new());
    }

    let mut users = Vec::new();
    for entry in fs::read_dir(&users_dir).context("Failed to read users directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            let content = fs::read_to_string(&path).unwrap_or_default();
            let name = if let Some(stripped) = content.strip_prefix("---\n").and_then(|s| s.split_once("\n---")) {
                if let Ok(fm) = serde_yaml::from_str::<std::collections::HashMap<String, String>>(stripped.0) {
                    fm.get("name").cloned().unwrap_or_else(|| stem.to_string())
                } else {
                    stem.to_string()
                }
            } else {
                stem.to_string()
            };
            users.push((stem.to_string(), name));
        }
    }
    users.sort();
    Ok(users)
}

/// Show a single user's info.
pub fn user_show(username: &str) -> Result<String> {
    let repo = kq_config::repo_path(None)?;
    let file_path = repo.join("users").join(format!("{}.md", username));
    if !file_path.exists() {
        bail!("User '{}' not found", username);
    }
    let content = fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;
    Ok(content)
}

// ---------------------------------------------------------------------------
// Frontmatter (immutable fields only — status/assignee from events)
// ---------------------------------------------------------------------------

/// Internal frontmatter matching the YAML structure exactly.
/// Only stores immutable fields — status/assignee come from events.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frontmatter {
    title: String,
    priority: Priority,
    created: NaiveDate,
}

impl From<&Task> for Frontmatter {
    fn from(task: &Task) -> Self {
        Self { title: task.title.clone(), priority: task.priority.clone(), created: task.created }
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Task status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Todo,
    InProgress,
    Review,
    Done,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Todo => write!(f, "todo"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Review => write!(f, "review"),
            Self::Done => write!(f, "done"),
        }
    }
}

impl FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.trim().to_lowercase().replace([' ', '-'], "_");
        match normalized.as_str() {
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "review" => Ok(Self::Review),
            "done" => Ok(Self::Done),
            _ => bail!("Invalid status '{}'. Valid values: todo, in_progress, review, done", s),
        }
    }
}

/// Task priority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::P0 => write!(f, "P0"),
            Self::P1 => write!(f, "P1"),
            Self::P2 => write!(f, "P2"),
            Self::P3 => write!(f, "P3"),
        }
    }
}

impl FromStr for Priority {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "P0" => Ok(Self::P0),
            "P1" => Ok(Self::P1),
            "P2" => Ok(Self::P2),
            "P3" => Ok(Self::P3),
            _ => bail!("Invalid priority '{}'. Valid values: P0, P1, P2, P3", s),
        }
    }
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A knowledge task with YAML frontmatter persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub assignee: String,
    pub created: NaiveDate,
    pub updated: NaiveDate,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the knowledge repo tasks directory path via kq_config.
fn tasks_dir() -> Result<PathBuf> {
    let repo = kq_config::repo_path(None)?;
    Ok(repo.join("tasks"))
}

/// Scan a tasks/ directory and return the maximum task ID number.
/// Returns 0 if the directory does not exist or contains no tasks.
pub(crate) fn max_task_number(tasks_dir: &Path) -> Result<u32> {
    if !tasks_dir.exists() {
        return Ok(0);
    }

    let mut max_num: u32 = 0;
    for entry in fs::read_dir(tasks_dir).context("Failed to read tasks directory")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if let Some(num_str) = name.strip_prefix("TASK-").and_then(|s| s.strip_suffix(".md"))
            && let Ok(num) = num_str.parse::<u32>()
            && num > max_num
        {
            max_num = num;
        }
    }
    Ok(max_num)
}

/// Format a number as a zero-padded 3-digit task ID (e.g. "TASK-001").
fn format_id(num: u32) -> String {
    format!("TASK-{:03}", num)
}

/// Parse YAML frontmatter from a markdown file content.
///
/// The content must start with `---`, then have YAML, then `---`, then body.
/// Returns (Frontmatter, body_string).
fn parse_frontmatter(content: &str) -> Result<(Frontmatter, String)> {
    let content = content.trim();
    if !content.starts_with("---") {
        bail!("Missing YAML frontmatter (expected file starting with '---')");
    }

    let after_first = content[3..].trim_start();

    // Find the closing "---" delimiter (must be preceded by newline)
    let end_pos = after_first
        .find("\n---\n")
        .ok_or_else(|| anyhow::anyhow!("Malformed YAML frontmatter: missing closing '---'"))?;

    let yaml_str = &after_first[..end_pos];
    let body = after_first[end_pos + 5..].trim_start().to_string();

    let frontmatter: Frontmatter = serde_yaml::from_str(yaml_str).context("Failed to parse YAML frontmatter")?;

    Ok((frontmatter, body))
}

/// Write a markdown file with YAML frontmatter.
fn write_task_file(path: &Path, frontmatter: &Frontmatter, id: &str, title: &str) -> Result<()> {
    let yaml_str = serde_yaml::to_string(frontmatter).context("Failed to serialize frontmatter")?;
    let yaml_str = yaml_str.trim_end();

    let content = format!("---\n{}\n---\n\n# {}: {}\n", yaml_str, id, title);

    fs::write(path, &content).with_context(|| format!("Failed to write task file: {}", path.display()))?;
    Ok(())
}

/// Read a task file and return the parsed Task.
/// Status and assignee are read from the frontmatter (source of truth),
/// events are used only for history/audit.
fn read_task_file(path: &Path) -> Result<Task> {
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read task file: {}", path.display()))?;
    let (frontmatter, _body) = parse_frontmatter(&content)?;
    let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

    // Parse assign, status, updated from frontmatter (source of truth)
    let mut assignee = String::new();
    let mut status = Status::Todo;
    let mut updated = frontmatter.created;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("assign: ") {
            assignee = val.trim().trim_matches(|c: char| c == '"' || c == '\'').to_string();
        } else if let Some(val) = line.strip_prefix("status: ") {
            status = val.trim().parse().unwrap_or(Status::Todo);
        } else if let Some(val) = line.strip_prefix("updated: ")
            && let Ok(d) = NaiveDate::parse_from_str(val.trim(), "%Y-%m-%d")
        {
            updated = d;
        }
    }

    Ok(Task {
        id,
        title: frontmatter.title,
        status,
        priority: frontmatter.priority,
        assignee,
        created: frontmatter.created,
        updated,
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a new task with the given title, priority, and optional assignee.
///
/// Creates `tasks/TASK-NNN.md` with minimal frontmatter (title, priority, created)
/// and a create event in `.kqs/events/TASK-NNN/`.
pub fn task_new(title: &str, status: Status, priority: Priority, assignee: &str) -> Result<Task> {
    let tasks_dir = tasks_dir()?;
    fs::create_dir_all(&tasks_dir).context("Failed to create tasks directory")?;

    let max_num = max_task_number(&tasks_dir)?;
    let next_num = max_num + 1;
    let id = format_id(next_num);
    let today = chrono::Utc::now().date_naive();

    // Minimal frontmatter — no status/assignee
    let frontmatter = Frontmatter { title: title.to_string(), priority: priority.clone(), created: today };

    let file_path = tasks_dir.join(format!("{}.md", id));
    write_task_file(&file_path, &frontmatter, &id, title)?;

    // Create event
    let repo = kq_config::repo_path(None)?;
    write_event(&repo, &id, "create", "todo")?;
    if !assignee.is_empty() {
        write_event(&repo, &id, "assign", assignee)?;
    }
    if status != Status::Todo {
        write_event(&repo, &id, "move", &status.to_string())?;
    }
    update_task_refs(&repo, &id)?;

    let state = replay_events(&crate::state_dir(&repo).join("events").join(&id)).unwrap_or_default();

    Ok(Task {
        id,
        title: title.to_string(),
        status: state.status,
        priority: priority.clone(),
        assignee: state.assignee,
        created: today,
        updated: NaiveDate::parse_from_str(&state.updated, "%Y%m%dT%H%M%SZ").unwrap_or(today),
    })
}

/// Update the task `.md` frontmatter with current state from events.
pub(crate) fn update_task_refs(repo: &Path, id: &str) -> Result<()> {
    let task_path = repo.join("tasks").join(format!("{}.md", id));
    if !task_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&task_path)?;

    let state = replay_events(&crate::state_dir(repo).join("events").join(id)).unwrap_or_default();

    // Extract immutable fields from existing frontmatter
    let (orig_title, orig_priority, orig_created) =
        if let Some(rest) = content.strip_prefix("---\n").and_then(|s| s.split_once("\n---\n")) {
            let (fm, _) = rest;
            let mut title = String::new();
            let mut priority = "P2".to_string();
            let mut created = String::new();
            for line in fm.lines() {
                if let Some(val) = line.strip_prefix("title: ") {
                    title = val.trim_matches('"').to_string();
                } else if let Some(val) = line.strip_prefix("priority: ") {
                    priority = val.to_string();
                } else if let Some(val) = line.strip_prefix("created: ") {
                    created = val.to_string();
                }
            }
            (title, priority, created)
        } else {
            (String::from("Task"), "P2".to_string(), chrono::Utc::now().date_naive().to_string())
        };

    let created_date =
        NaiveDate::parse_from_str(&orig_created, "%Y-%m-%d").unwrap_or_else(|_| chrono::Utc::now().date_naive());
    let priority_enum: Priority = orig_priority.parse().unwrap_or(Priority::P2);

    let updated_str = if state.updated.len() >= 8 {
        Some(format!("{}-{}-{}", &state.updated[0..4], &state.updated[4..6], &state.updated[6..8]))
    } else if state.updated.is_empty() {
        None
    } else {
        Some(state.updated.clone())
    };

    // Build frontmatter using serde (always valid YAML)
    let fm = FrontmatterWithState {
        title: orig_title.clone(),
        task_id: id.to_string(),
        priority: priority_enum,
        created: created_date,
        updated: updated_str,
        assign: if state.assignee.is_empty() { None } else { Some(state.assignee.clone()) },
        status: state.status.to_string(),
    };

    let yaml_str = serde_yaml::to_string(&fm).context("Failed to serialize frontmatter")?;
    let yaml_str = yaml_str.trim_end();
    let new_content = format!("---\n{}\n---\n\n# {}: {}\n", yaml_str, id, orig_title);

    if new_content != content {
        fs::write(&task_path, &new_content).ok();
    }
    Ok(())
}

/// Extended frontmatter for writing (includes computed state).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrontmatterWithState {
    title: String,
    #[serde(rename = "task_id")]
    task_id: String,
    priority: Priority,
    created: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assign: Option<String>,
    status: String,
}

/// Assign a task to a user. Creates an assign event.
/// Assign a task to a user. Creates an assign event and updates @refs in the .md body.
pub fn task_assign(id: &str, username: &str) -> Result<Task> {
    let repo = kq_config::repo_path(None)?;
    write_event(&repo, id, "assign", username)?;
    update_task_refs(&repo, id)?;
    task_status_read(id)
}

/// Move a task to a new status. Creates a move event and updates @refs in the .md body.
pub fn task_move(id: &str, new_status: Status) -> Result<Task> {
    let repo = kq_config::repo_path(None)?;
    write_event(&repo, id, "move", &new_status.to_string())?;
    update_task_refs(&repo, id)?;
    task_status_read(id)
}

/// List all tasks, optionally filtered by status and/or priority.
///
/// Returns tasks sorted by ID descending (newest first).
/// Returns an empty Vec (not an error) if the tasks directory does not exist
/// or no tasks match the filters.
pub fn task_list(status_filter: Option<Status>, priority_filter: Option<Priority>) -> Result<Vec<Task>> {
    let tasks_dir = tasks_dir()?;

    if !tasks_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tasks: Vec<Task> = Vec::new();

    for entry in fs::read_dir(&tasks_dir).context("Failed to read tasks directory")? {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files with TASK- prefix
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !file_stem.starts_with("TASK-") {
            continue;
        }

        match read_task_file(&path) {
            Ok(task) => {
                // Apply filters
                if let Some(status) = &status_filter
                    && &task.status != status
                {
                    continue;
                }
                if let Some(priority) = &priority_filter
                    && &task.priority != priority
                {
                    continue;
                }
                tasks.push(task);
            }
            Err(e) => {
                eprintln!("Warning: Skipping {}: {}", path.display(), e);
            }
        }
    }

    tasks.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(tasks)
}

/// Update the status of a task by creating a move event.
pub fn task_status(id: &str, new_status: Status) -> Result<Task> {
    let tasks_dir = tasks_dir()?;
    let file_path = tasks_dir.join(format!("{}.md", id));
    if !file_path.exists() {
        bail!("Task '{}' not found at {}", id, file_path.display());
    }

    let repo = kq_config::repo_path(None)?;
    write_event(&repo, id, "move", &new_status.to_string())?;
    update_task_refs(&repo, id)?;
    task_status_read(id)
}

/// Show a single task by ID, computing state from events.
pub fn task_status_read(id: &str) -> Result<Task> {
    let tasks_dir = tasks_dir()?;
    let file_path = tasks_dir.join(format!("{}.md", id));
    if !file_path.exists() {
        bail!("Task '{}' not found", id);
    }
    read_task_file(&file_path)
}

/// Show a single task by ID (e.g. "TASK-001").
pub fn task_show(id: &str) -> Result<Task> {
    let tasks_dir = tasks_dir()?;
    let file_path = tasks_dir.join(format!("{}.md", id));
    if !file_path.exists() {
        bail!("Task '{}' not found", id);
    }
    read_task_file(&file_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    /// Global mutex to serialize tests that change the process-wide current
    /// working directory. Without this, parallel test execution would race
    /// on `std::env::set_current_dir` / `kq_config::repo_path`.
    /// We ignore poisoning so a panicked test does not permanently lock the
    /// mutex for subsequent tests.
    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn cwd_lock() -> &'static Mutex<()> {
        CWD_LOCK.get_or_init(|| Mutex::new(()))
    }

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Test helper that sets up a temporary knowledge repository environment.
    /// Acquires the global CWD_LOCK to prevent races with other tests.
    struct TestEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        _temp_dir: tempfile::TempDir,
        prev_cwd: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            // Ignore poisoning so a previous panic does not cascade
            let guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());

            TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let temp_dir = tempfile::TempDir::new().expect("Failed to create TempDir");
            let dir = temp_dir.path().to_path_buf();

            // Write knowledge.toml that points to this directory
            let toml_content = format!("knowledge_path = \"{}\"\n", dir.to_string_lossy().replace('\\', "/"));
            fs::write(dir.join("knowledge.toml"), &toml_content).expect("Failed to write knowledge.toml");

            let prev_cwd = std::env::current_dir().expect("Failed to get current dir");
            std::env::set_current_dir(&dir).expect("Failed to set current dir");

            Self { _guard: guard, _temp_dir: temp_dir, prev_cwd }
        }

        fn tasks_path(&self) -> PathBuf {
            let dir = std::env::current_dir().expect("Failed to get current dir");
            dir.join("tasks")
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.prev_cwd).ok();
        }
    }

    // -----------------------------------------------------------------------
    // Status enum tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_status_display_and_from_str() {
        assert_eq!(Status::Todo.to_string(), "todo");
        assert_eq!(Status::InProgress.to_string(), "in_progress");
        assert_eq!(Status::Review.to_string(), "review");
        assert_eq!(Status::Done.to_string(), "done");

        assert_eq!("todo".parse::<Status>().unwrap(), Status::Todo);
        assert_eq!("in_progress".parse::<Status>().unwrap(), Status::InProgress);
        assert_eq!("review".parse::<Status>().unwrap(), Status::Review);
        assert_eq!("done".parse::<Status>().unwrap(), Status::Done);
    }

    #[test]
    fn test_status_validation() {
        // Valid statuses should parse successfully
        assert!("todo".parse::<Status>().is_ok());
        assert!("in_progress".parse::<Status>().is_ok());
        assert!("review".parse::<Status>().is_ok());
        assert!("done".parse::<Status>().is_ok());
    }

    #[test]
    fn test_invalid_status_error() {
        let err = "invalid".parse::<Status>().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Invalid status 'invalid'"));
        assert!(msg.contains("Valid values: todo, in_progress, review, done"));
    }

    // -----------------------------------------------------------------------
    // Priority enum tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_priority_display_and_from_str() {
        assert_eq!(Priority::P0.to_string(), "P0");
        assert_eq!(Priority::P1.to_string(), "P1");
        assert_eq!(Priority::P2.to_string(), "P2");
        assert_eq!(Priority::P3.to_string(), "P3");

        assert_eq!("P0".parse::<Priority>().unwrap(), Priority::P0);
        assert_eq!("P1".parse::<Priority>().unwrap(), Priority::P1);
        assert_eq!("P2".parse::<Priority>().unwrap(), Priority::P2);
        assert_eq!("P3".parse::<Priority>().unwrap(), Priority::P3);
    }

    #[test]
    fn test_priority_validation() {
        assert!("P0".parse::<Priority>().is_ok());
        assert!("P1".parse::<Priority>().is_ok());
        assert!("P2".parse::<Priority>().is_ok());
        assert!("P3".parse::<Priority>().is_ok());
    }

    #[test]
    fn test_invalid_priority_error() {
        let err = "P5".parse::<Priority>().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Invalid priority 'P5'"));
        assert!(msg.contains("Valid values: P0, P1, P2, P3"));
    }

    // -----------------------------------------------------------------------
    // task_new tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_task_new_creates_file() -> Result<()> {
        let env = TestEnv::new();
        let task = task_new("Test task", Status::Todo, Priority::P1, "@alice")?;
        assert_eq!(task.id, "TASK-001");
        assert_eq!(task.title, "Test task");
        assert_eq!(task.status, Status::Todo);
        assert_eq!(task.priority, Priority::P1);
        assert_eq!(task.assignee, "@alice");
        assert!(env.tasks_path().join("TASK-001.md").exists());
        Ok(())
    }

    #[test]
    fn test_task_new_auto_numbering_empty() -> Result<()> {
        let _env = TestEnv::new();
        let task = task_new("First task", Status::Todo, Priority::P2, "")?;
        assert_eq!(task.id, "TASK-001");
        Ok(())
    }

    #[test]
    fn test_task_new_auto_numbering_increment() -> Result<()> {
        let _env = TestEnv::new();
        let t1 = task_new("Task 1", Status::Todo, Priority::P2, "")?;
        assert_eq!(t1.id, "TASK-001");
        let t2 = task_new("Task 2", Status::InProgress, Priority::P1, "@bob")?;
        assert_eq!(t2.id, "TASK-002");
        let t3 = task_new("Task 3", Status::Done, Priority::P0, "@carol")?;
        assert_eq!(t3.id, "TASK-003");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // task_list tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_task_list_empty() -> Result<()> {
        let _env = TestEnv::new();
        let tasks = task_list(None, None)?;
        assert!(tasks.is_empty());
        Ok(())
    }

    #[test]
    fn test_task_list_with_tasks() -> Result<()> {
        let _env = TestEnv::new();
        task_new("Task A", Status::Todo, Priority::P2, "@alice")?;
        task_new("Task B", Status::InProgress, Priority::P1, "@bob")?;

        let tasks = task_list(None, None)?;
        assert_eq!(tasks.len(), 2);
        // Sorted by ID descending: TASK-002 first, then TASK-001
        assert_eq!(tasks[0].id, "TASK-002");
        assert_eq!(tasks[1].id, "TASK-001");
        Ok(())
    }

    #[test]
    fn test_task_list_filter_by_status() -> Result<()> {
        let _env = TestEnv::new();
        task_new("Task A", Status::Todo, Priority::P2, "@alice")?;
        task_new("Task B", Status::InProgress, Priority::P1, "@bob")?;
        task_new("Task C", Status::Todo, Priority::P0, "@carol")?;

        let todo_tasks = task_list(Some(Status::Todo), None)?;
        assert_eq!(todo_tasks.len(), 2);
        assert!(todo_tasks.iter().all(|t| t.status == Status::Todo));

        let in_progress_tasks = task_list(Some(Status::InProgress), None)?;
        assert_eq!(in_progress_tasks.len(), 1);
        assert_eq!(in_progress_tasks[0].title, "Task B");

        let done_tasks = task_list(Some(Status::Done), None)?;
        assert!(done_tasks.is_empty());
        Ok(())
    }

    #[test]
    fn test_task_list_filter_by_priority() -> Result<()> {
        let _env = TestEnv::new();
        task_new("Task A", Status::Todo, Priority::P2, "@alice")?;
        task_new("Task B", Status::InProgress, Priority::P1, "@bob")?;
        task_new("Task C", Status::Todo, Priority::P2, "@carol")?;

        let p2_tasks = task_list(None, Some(Priority::P2))?;
        assert_eq!(p2_tasks.len(), 2);
        assert!(p2_tasks.iter().all(|t| t.priority == Priority::P2));

        let p1_tasks = task_list(None, Some(Priority::P1))?;
        assert_eq!(p1_tasks.len(), 1);
        assert_eq!(p1_tasks[0].title, "Task B");

        let p3_tasks = task_list(None, Some(Priority::P3))?;
        assert!(p3_tasks.is_empty());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // task_status test
    // -----------------------------------------------------------------------

    #[test]
    fn test_task_status_update() -> Result<()> {
        let _env = TestEnv::new();
        let task = task_new("Status update test", Status::Todo, Priority::P2, "@alice")?;
        assert_eq!(task.status, Status::Todo);

        let updated = task_status("TASK-001", Status::InProgress)?;
        assert_eq!(updated.status, Status::InProgress);
        assert_eq!(updated.id, "TASK-001");

        // Verify by re-reading
        let shown = task_show("TASK-001")?;
        assert_eq!(shown.status, Status::InProgress);
        assert_eq!(shown.updated, updated.updated);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // task_show tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_task_show() -> Result<()> {
        let _env = TestEnv::new();
        task_new("Show me", Status::Review, Priority::P0, "@dave")?;

        let task = task_show("TASK-001")?;
        assert_eq!(task.id, "TASK-001");
        assert_eq!(task.title, "Show me");
        assert_eq!(task.status, Status::Review);
        assert_eq!(task.priority, Priority::P0);
        assert_eq!(task.assignee, "@dave");
        Ok(())
    }

    #[test]
    fn test_task_show_not_found() {
        let _env = TestEnv::new();
        let result = task_show("TASK-999");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("TASK-999"));
    }
}

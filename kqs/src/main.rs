use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

/// kqs — Git-native knowledge platform.
///
/// Initialize, watch, search, and manage a local knowledge repository
/// backed by git.
#[derive(Parser)]
#[command(name = "kqs", author, version, about, long_about = None)]
struct Cli {
    /// Run in dev mode (project repo context)
    #[arg(long, global = true)]
    dev: bool,
    /// Run in doc mode (knowledge repo context)
    #[arg(long, global = true)]
    doc: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Initialize a new knowledge repository
    Init(InitArgs),
    /// Search indexed documents
    Search(SearchArgs),
    /// Manage tasks
    Task(TaskArgs),
    /// Manage users
    User(UserArgs),
    /// Watch for file changes and auto-commit
    Watch(WatchArgs),
    /// Push changes to remote with auto-generated README
    Push(PushArgs),
    /// Resolve merge conflicts
    Conflict(ConflictArgs),
    /// Regenerate the README kanban board
    Readme,
    /// Ask a question using LLM
    Ask(AskArgs),
    /// Manage documentation
    Doc(DocArgs),
    /// Manage TypeSpec models
    Typespec(TypespecArgs),
    /// Generate a screen design document
    Screen(ScreenArgs),
    /// Generate a user flow document
    Userflow(UserflowArgs),
    /// Run checks (traceability, orphans)
    Check(CheckArgs),
    /// Handle a kq:// protocol URL
    HandleUrl { url: String },
    /// Install the kq:// protocol handler on this system
    InstallProtocol,
    /// LLM-oriented help: complete system prompt for AI assistants
    LlmHelp,
}

#[derive(Parser)]
struct InitArgs {
    /// Use vector search (requires embedding model)
    #[arg(long, value_name = "PATH")]
    path: Option<PathBuf>,

    /// Git remote URL to add as origin
    #[arg(long, value_name = "URL")]
    remote: Option<String>,

    /// Reinitialize an existing repository
    #[arg(long, default_value_t = false)]
    force: bool,
}

#[derive(Parser)]
struct SearchArgs {
    /// Search query (FTS5 full-text search by default)
    query: String,

    /// Maximum number of results (default: 10)
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Use full-text search (default, on by default)
    #[arg(long, default_value_t = true)]
    fts: bool,
    /// Path to the knowledge repository (auto-detected by default)
    #[arg(long, value_name = "PATH")]
    repo: Option<PathBuf>,
    /// Use vector search (requires embedding model)
    #[arg(long, default_value_t = false)]
    vector: bool,
}

#[derive(Parser)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Parser)]
enum TaskCommand {
    /// Create a new task
    New(NewTaskArgs),
    /// List all tasks
    List(ListTaskArgs),
    /// Update task status
    Status(StatusTaskArgs),
    /// Show task details
    Show(ShowTaskArgs),
    /// Assign a task to a user
    Assign(AssignTaskArgs),
    /// Move task to a new status
    Move(MoveTaskArgs),
}

#[derive(Parser)]
struct AssignTaskArgs {
    /// Task ID (e.g. TASK-001)
    id: String,
    /// Username
    user: String,
}

#[derive(Parser)]
struct MoveTaskArgs {
    /// Task ID (e.g. TASK-001)
    id: String,
    /// New status [todo, in_progress, review, done]
    status: String,
}

#[derive(Parser)]
struct UserArgs {
    #[command(subcommand)]
    command: UserCommand,
}

#[derive(Parser)]
enum UserCommand {
    /// Create a new user
    Create(UserCreateArgs),
    /// List all users
    List,
    /// Show user details
    Show { username: String },
}

#[derive(Parser)]
struct UserCreateArgs {
    /// Username
    username: String,
    /// Display name
    #[arg(long)]
    name: Option<String>,
}

#[derive(Parser)]
struct WatchArgs {
    /// Debounce seconds before auto-commit (overrides config)
    #[arg(long, default_value_t = 60)]
    debounce_secs: u64,
    /// Enable trace daemon: rebuild graph and scan projects on each cycle
    #[arg(long, default_value_t = false)]
    trace: bool,
}

#[derive(Parser)]
struct NewTaskArgs {
    /// Task title
    #[arg(short, long)]
    title: Option<String>,
    /// Task priority [P0, P1, P2, P3]
    #[arg(short, long, default_value = "P2")]
    priority: String,
    /// Assignee
    #[arg(short, long, default_value = "")]
    assignee: String,
    /// Task status [todo, in_progress, review, done]
    #[arg(short, long, default_value = "todo")]
    status: String,
}

#[derive(Parser)]
struct ListTaskArgs {
    /// Filter by status [todo, in_progress, review, done]
    #[arg(long)]
    status: Option<String>,
    /// Filter by priority [P0, P1, P2, P3]
    #[arg(long)]
    priority: Option<String>,
}

#[derive(Parser)]
struct StatusTaskArgs {
    /// Task ID (e.g. TASK-001)
    id: String,
    /// New status [todo, in_progress, review, done]
    status: String,
}

#[derive(Parser)]
struct ShowTaskArgs {
    /// Task ID (e.g. TASK-001)
    id: String,
}

#[derive(Parser)]
struct PushArgs {
    /// Path to the knowledge repository (auto-detected by default)
    #[arg(long, value_name = "PATH")]
    path: Option<PathBuf>,

    /// Show what would be pushed without actually pushing
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Skip README generation before push
    #[arg(long, default_value_t = false)]
    no_readme: bool,
}

#[derive(Parser)]
struct ConflictArgs {
    #[command(subcommand)]
    command: ConflictCommand,
}

#[derive(Parser)]
enum ConflictCommand {
    /// List conflicted files
    List,
    /// Show conflict content for a file
    Show {
        /// File path (relative to repo root)
        file: String,
    },
    /// Resolve a conflict using a strategy
    Resolve {
        /// File path (relative to repo root)
        file: String,
        /// Strategy: ours or theirs
        strategy: String,
    },
}

#[derive(Parser)]
struct AskArgs {
    /// Query to ask the LLM
    #[arg(required = true)]
    query: Vec<String>,

    /// Insert the response into a file
    #[arg(long, value_name = "PATH")]
    insert: Option<PathBuf>,
}

#[derive(Parser)]
struct DocArgs {
    #[command(subcommand)]
    command: DocCommand,
}

#[derive(Parser)]
enum DocCommand {
    /// List all documents
    List,
    /// Create a new document
    New {
        /// Document type (bft, brd, frd, nfr, adr, rfc, tz, idea, user_story, glossary)
        doc_type: String,
        /// Document title
        title: String,
    },
    /// Show available templates
    Template {
        /// List all template types
        #[arg(long, default_value_t = false)]
        list: bool,
    },
}

#[derive(Parser)]
struct TypespecArgs {
    #[command(subcommand)]
    command: TypespecCommand,
}

#[derive(Parser)]
enum TypespecCommand {
    /// Create a new TypeSpec model
    New {
        /// Model name
        name: String,
    },
    /// List all TypeSpec models
    List,
}

#[derive(Parser)]
struct ScreenArgs {
    /// Screen title
    title: String,
}

#[derive(Parser)]
struct UserflowArgs {
    /// User flow title
    title: String,
}

#[derive(Parser)]
struct CheckArgs {
    #[command(subcommand)]
    command: CheckCommand,
}

#[derive(Parser)]
enum CheckCommand {
    /// Run full traceability report
    Traceability,
    /// Run deep traceability with full chain coverage
    TraceabilityDeep {
        #[arg(long, default_value_t = false)]
        deep: bool,
        #[arg(long, value_name = "CHAIN")]
        chain: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Scan project repos for code anchors
    Scan {
        #[arg(long, default_value_t = false)]
        rebuild: bool,
    },
    /// Show stale-link notifications, optionally filtered by time
    Notify {
        #[arg(long, value_name = "PERIOD")]
        since: Option<String>,
    },
    /// Find orphaned models with no doc links
    Orphans,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Mode detection
    let mode = if cli.dev {
        kq_core::KqMode::Dev
    } else if cli.doc {
        kq_core::KqMode::Doc
    } else {
        kq_core::detect_mode(&std::env::current_dir()?)
    };
    kq_core::set_mode(mode);

    // Gating: doc-only commands check
    if kq_core::is_dev() {
        match &cli.command {
            Command::Doc(_) | Command::Readme | Command::Typespec(_) => {
                anyhow::bail!("This command requires doc mode (run with --doc or from knowledge repo)");
            }
            _ => {}
        }
    }

    // Auto-detect: if handle-url and no repo marker, save current repo
    match cli.command {
        Command::Init(args) => {
            kq_core::init::init(args.path.clone(), args.remote, args.force)?;
            let target =
                args.path.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            println!("Initialized empty knowledge repository at {}", target.display());
        }
        Command::Search(args) => {
            let repo_path = kq_config::repo_path(args.repo.as_ref().map(|p| p.to_str().unwrap_or_default()))?;
            let db_path = repo_path.join(".kq/knowledge.db");

            if !db_path.exists() {
                anyhow::bail!(
                    "Knowledge database not found at {}. Run `kqs init` first or use --repo to specify a different path.",
                    db_path.display()
                );
            }

            kq_core::db::init_db(&db_path)?;
            let conn = kq_core::db::get_db()?;
            // Ensure index is fresh
            if args.vector {
                let mut model = kq_embeddings::EmbeddingModel::new(&repo_path.join(".kq/model-cache"))?;
                match model.load() {
                    Ok(()) => {
                        let emb = model.embed(&args.query)?;
                        let results = kq_core::vector::hybrid_search(&conn, &args.query, &emb, args.limit, args.limit)?;
                        for r in &results {
                            println!("  {:6.2}  {}", r.score, r.path);
                        }
                    }
                    Err(e) => {
                        eprintln!("Vector search unavailable: {e}");
                        eprintln!("Model will be downloaded on first vector search.");
                    }
                }
                return Ok(());
            }

            if args.vector {
                let mut model = kq_embeddings::EmbeddingModel::new(&repo_path.join(".kq/model-cache"))?;
                match model.load() {
                    Ok(()) => {
                        let emb = model.embed(&args.query)?;
                        let results = kq_core::vector::hybrid_search(&conn, &args.query, &emb, args.limit, args.limit)?;
                        for r in &results {
                            println!("  {:6.2}  {}", r.score, r.path);
                        }
                    }
                    Err(e) => {
                        eprintln!("Vector search unavailable: {e}");
                        eprintln!("Model will be downloaded on first vector search.");
                    }
                }
                return Ok(());
            }

            let results = kq_core::search::search_fts(&conn, &args.query, args.limit)?;

            if results.is_empty() {
                println!("No results found for query: {}", args.query);
            } else {
                println!("Search results for: \"{}\"\n", args.query);
                for (i, result) in results.iter().enumerate() {
                    println!("{:>3}. {:.2}  {}\n     Context: {}\n", i + 1, result.score, result.path, result.context);
                }
            }
        }
        Command::Watch(args) => {
            let config = load_config_for_watch()?;

            let mut config = config;
            if args.debounce_secs != 60 {
                config.watcher.debounce_secs = args.debounce_secs;
            }

            let repo_path = expand_tilde(&config.knowledge_path);
            let db_path = repo_path.join(".kq/knowledge.db");
            if db_path.exists() {
                kq_core::db::init_db(&db_path).context("Failed to initialize knowledge database")?;
            } else {
                eprintln!("[kqs] No database found at {} — watch will skip indexing", db_path.display());
            }

            // Trace daemon: initial cycle
            if args.trace {
                println!("[kqs] Trace daemon: rebuilding graph...");
                if let Err(e) = kq_core::check::rebuild_trace_graph(&repo_path) {
                    eprintln!("[kqs] Trace rebuild failed: {e}");
                }
                println!("[kqs] Trace daemon: scanning projects...");
                if let Err(e) = kq_core::check::scan_projects(&repo_path) {
                    eprintln!("[kqs] Project scan failed: {e}");
                }
                if let Err(e) = kq_core::check::print_stale_links() {
                    eprintln!("[kqs] Stale link check failed: {e}");
                }
                println!("[kqs] Trace daemon: active — will re-check on each commit cycle");
            }

            let rt = tokio::runtime::Runtime::new().context("Failed to start async runtime for watcher")?;
            rt.block_on(kq_core::watcher::start_watch(&config))?;

            // After watcher stops: final trace
            if args.trace {
                println!("\n[kqs] Trace daemon: final scan...");
                let _ = kq_core::check::rebuild_trace_graph(&repo_path);
                let _ = kq_core::check::scan_projects(&repo_path);
                let _ = kq_core::check::print_stale_links();
            }
        }
        Command::User(args) => match args.command {
            UserCommand::Create(args) => {
                kq_core::task::user_create(&args.username, args.name.as_deref())?;
                println!("Created user: {}", args.username);
            }
            UserCommand::List => {
                let users = kq_core::task::user_list()?;
                if users.is_empty() {
                    println!("No users found.");
                } else {
                    println!("{:<20} Name", "Username");
                    println!("{:<20} ----", "--------");
                    for (username, name) in &users {
                        println!("{:<20} {}", username, name);
                    }
                }
            }
            UserCommand::Show { username } => {
                let content = kq_core::task::user_show(&username)?;
                println!("{}", content);
            }
        },
        Command::Task(args) => {
            use kq_core::task::{Priority, Status};
            match args.command {
                TaskCommand::New(args) => {
                    let title = match args.title {
                        Some(t) => t,
                        None => {
                            print!("Title: ");
                            std::io::Write::flush(&mut std::io::stdout()).context("Failed to flush stdout")?;
                            let mut buf = String::new();
                            std::io::stdin().read_line(&mut buf).context("Failed to read title from stdin")?;
                            buf.trim().to_string()
                        }
                    };

                    let priority: Priority = args.priority.parse().context("Invalid priority value")?;
                    let status: Status = args.status.parse().context("Invalid status value")?;

                    let task = kq_core::task::task_new(&title, status, priority, &args.assignee)?;
                    println!("Created {}: {}", task.id, task.title);
                }
                TaskCommand::List(args) => {
                    let status_filter: Option<Status> = match args.status {
                        Some(s) => Some(s.parse().context("Invalid status value")?),
                        None => None,
                    };
                    let priority_filter: Option<Priority> = match args.priority {
                        Some(p) => Some(p.parse().context("Invalid priority value")?),
                        None => None,
                    };

                    let tasks = kq_core::task::task_list(status_filter, priority_filter)?;

                    if tasks.is_empty() {
                        println!("No tasks found.");
                    } else {
                        println!(
                            "{:<12} {:<10} {:<25} {:<12} {:<10} Updated",
                            "Status", "ID", "Title", "Assignee", "Priority"
                        );
                        println!(
                            "{:<12} {:<10} {:<25} {:<12} {:<10} -------",
                            "------", "--", "-----", "--------", "--------"
                        );
                        for task in &tasks {
                            println!(
                                "{:<12} {:<10} {:<25} {:<12} {:<10} {}",
                                task.status, task.id, task.title, task.assignee, task.priority, task.updated,
                            );
                        }
                    }
                }
                TaskCommand::Status(args) => {
                    let new_status: Status = args.status.parse().context("Invalid status value")?;
                    let task = kq_core::task::task_status(&args.id, new_status)?;
                    println!("Updated {} to status '{}'", task.id, task.status);
                }
                TaskCommand::Show(args) => {
                    let task = kq_core::task::task_show(&args.id)?;
                    println!("ID:       {}", task.id);
                    println!("Title:    {}", task.title);
                    println!("Status:   {}", task.status);
                    println!("Priority: {}", task.priority);
                    println!("Assignee: {}", task.assignee);
                    println!("Created:  {}", task.created);
                    println!("Updated:  {}", task.updated);
                }
                TaskCommand::Assign(args) => {
                    let task = kq_core::task::task_assign(&args.id, &args.user)?;
                    println!("Assigned {} to {}", task.id, task.assignee);
                }
                TaskCommand::Move(args) => {
                    let new_status: Status = args.status.parse().context("Invalid status value")?;
                    let task = kq_core::task::task_move(&args.id, new_status)?;
                    println!("Moved {} to status '{}'", task.id, task.status);
                }
            }
        }
        Command::Push(args) => {
            let repo_path = resolve_repo_path(args.path.as_deref())?;
            kq_core::push::push(&repo_path, args.dry_run, args.no_readme)?;
        }
        Command::Conflict(args) => {
            let repo_path = resolve_repo_path(None)?;
            match args.command {
                ConflictCommand::List => {
                    let files = kq_core::conflict::list(&repo_path)?;
                    if files.is_empty() {
                        println!("No conflicts found.");
                    } else {
                        println!("Conflicted files:");
                        for file in &files {
                            println!("  M  {file}");
                        }
                    }
                }
                ConflictCommand::Show { file } => {
                    let content = kq_core::conflict::show(&repo_path, &file)?;
                    println!("{content}");
                }
                ConflictCommand::Resolve { file, strategy } => match strategy.as_str() {
                    "ours" => {
                        kq_core::conflict::resolve_ours(&repo_path, &file)?;
                        println!("Resolved {file} using 'ours' strategy");
                    }
                    "theirs" => {
                        kq_core::conflict::resolve_theirs(&repo_path, &file)?;
                        println!("Resolved {file} using 'theirs' strategy");
                    }
                    _ => {
                        anyhow::bail!("Unknown strategy '{}'. Use 'ours' or 'theirs'.", strategy);
                    }
                },
            }
        }
        Command::Readme => {
            let repo_path = resolve_repo_path(None)?;
            let readme_path = repo_path.join("README.md");
            if !readme_path.exists() {
                let initial = "# Project\n\n<!-- kq:start -->\n<!-- kq:end -->\n";
                std::fs::write(&readme_path, initial)
                    .with_context(|| format!("Failed to create {}", readme_path.display()))?;
                eprintln!("[kqs] Created README.md at {}", readme_path.display());
            }
            kq_core::readme_gen::generate(&repo_path)?;
            println!("Regenerated README.md at {}", readme_path.display());
        }
        Command::Ask(args) => {
            let query = args.query.join(" ");
            let repo_path = resolve_repo_path(None)?;
            let config_path = repo_path.join("knowledge.toml");

            if !config_path.exists() {
                anyhow::bail!("No knowledge.toml found. Run `kqs init` first.");
            }

            let config = kq_config::KnowledgeConfig::load(&config_path)?;
            let llm_config = config
                .llm
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("LLM not configured. Add [llm] section to knowledge.toml.\nExample:\n  [llm]\n  provider = \"ollama\"\n  model = \"llama3\"\n  endpoint = \"http://localhost:11434\""))?;

            // Get context from search
            let db_path = repo_path.join(".kq/knowledge.db");
            let mut context: Vec<String> = Vec::new();
            if db_path.exists() {
                kq_core::db::init_db(&db_path)?;
                let conn = kq_core::db::get_db()?;
                let results = kq_core::search::search_fts(&conn, &query, 5)?;
                for r in &results {
                    context.push(format!("File: {}\nRelevance: {:.2}\nContext: {}", r.path, r.score, r.context));
                }
            }

            if !context.is_empty() {
                eprintln!("[kqs] Using {} relevant documents as context", context.len());
            } else {
                eprintln!("[kqs] No context documents found — answering without context");
            }

            // Create provider
            let provider: Box<dyn kq_llm::LlmProvider> = match llm_config.provider.as_str() {
                "ollama" => {
                    let endpoint =
                        if llm_config.endpoint.is_empty() { None } else { Some(llm_config.endpoint.clone()) };
                    Box::new(kq_llm::ollama::OllamaProvider::new(llm_config.model.clone(), endpoint))
                }
                "openai" => Box::new(kq_llm::openai::OpenAiProvider::new(
                    llm_config.model.clone(),
                    Some(llm_config.endpoint.clone()),
                    llm_config.api_key.clone(),
                )?),
                "anthropic" => Box::new(kq_llm::anthropic::AnthropicProvider::new(
                    llm_config.model.clone(),
                    llm_config.api_key.clone(),
                )?),
                _ => anyhow::bail!("Unknown LLM provider: {}", llm_config.provider),
            };

            // Run async streaming
            let rt = tokio::runtime::Runtime::new().context("Failed to start async runtime for LLM")?;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            let prompt = query.clone();
            rt.spawn(async move {
                if let Err(e) = provider.ask_stream(&prompt, &context, tx).await {
                    eprintln!("\n[kqs] LLM error: {e}");
                }
            });

            let mut response = String::new();
            while let Some(chunk) = rt.block_on(rx.recv()) {
                print!("{chunk}");
                std::io::Write::flush(&mut std::io::stdout())?;
                response.push_str(&chunk);
            }
            println!();

            if let Some(insert_path) = &args.insert {
                std::fs::write(insert_path, &response)
                    .with_context(|| format!("Failed to write to {}", insert_path.display()))?;
                eprintln!("[kqs] Response written to {}", insert_path.display());
            }
        }
        Command::Doc(args) => {
            let repo_path = resolve_repo_path(None)?;
            match args.command {
                DocCommand::List => {
                    let docs = kq_core::docs::list_docs(&repo_path)?;
                    if docs.is_empty() {
                        println!("No documents found.");
                    } else {
                        println!("Documents:");
                        for (category, path) in &docs {
                            println!("  [{category}] {path}");
                        }
                    }
                }
                DocCommand::New { doc_type, title } => {
                    let path = kq_core::docs::generate_doc(&repo_path, &doc_type, &title)?;
                    println!("Created document: {path}");
                }
                DocCommand::Template { list } => {
                    if list {
                        let templates = kq_core::docs::templates_list(Some(&repo_path));
                        println!("Available templates:");
                        for t in &templates {
                            println!("  {t}");
                        }
                    } else {
                        let templates = kq_core::docs::templates_list(Some(&repo_path));
                        println!("Use `kqs doc template --list` to see all {} templates.", templates.len());
                    }
                }
            }
        }
        Command::Typespec(args) => {
            let repo_path = resolve_repo_path(None)?;
            match args.command {
                TypespecCommand::New { name } => {
                    let path = kq_core::typespec::new_type(&repo_path, &name)?;
                    println!("Created TypeSpec model: {path}");
                    kq_core::typespec::init_main_tsp(&repo_path)?;
                    println!("Updated TypeSpec/main.tsp");
                }
                TypespecCommand::List => {
                    let models = kq_core::typespec::list_types(&repo_path)?;
                    if models.is_empty() {
                        println!("No TypeSpec models found.");
                    } else {
                        println!("{:<20} {:<30} Doc Refs", "Name", "File");
                        println!("{:<20} {:<30} --------", "----", "----");
                        for model in &models {
                            println!(
                                "{:<20} {:<30} {}",
                                model.name,
                                model.file,
                                if model.doc_refs.is_empty() { "—".to_string() } else { model.doc_refs.join(", ") }
                            );
                        }
                    }
                }
            }
        }
        Command::Screen(args) => {
            let repo_path = resolve_repo_path(None)?;
            kq_core::docs::generate_doc(&repo_path, "screen", &args.title)?;
            println!("Screen design document created.");
        }
        Command::Userflow(args) => {
            let repo_path = resolve_repo_path(None)?;
            kq_core::docs::generate_doc(&repo_path, "userflow", &args.title)?;
            println!("User flow document created.");
        }
        Command::Check(args) => {
            let repo_path = resolve_repo_path(None)?;
            let db_path = repo_path.join(".kq/knowledge.db");
            if db_path.exists() {
                let _ = kq_core::db::init_db(&db_path);
            }
            match args.command {
                CheckCommand::Traceability => {
                    kq_core::check::traceability(&repo_path)?;
                }
                CheckCommand::TraceabilityDeep { deep, chain, json } => {
                    let report = kq_core::check::traceability_deep(&repo_path, deep, chain.as_deref(), json)?;
                    if json {
                        println!("{}", report);
                    }
                }
                CheckCommand::Scan { rebuild } => {
                    if rebuild {
                        kq_core::check::rebuild_trace_graph(&repo_path)?;
                    }
                    kq_core::check::scan_projects(&repo_path)?;
                }
                CheckCommand::Notify { since } => {
                    kq_core::check::print_stale_notifications(since.as_deref())?;
                }
                CheckCommand::Orphans => {
                    let orphans = kq_core::check::orphans(&repo_path)?;
                    if orphans.is_empty() {
                        println!("No orphans found.");
                    } else {
                        println!("Orphaned objects (no doc links):");
                        for name in &orphans {
                            println!("  - {name}");
                        }
                    }
                }
            }
        }
        Command::HandleUrl { url } => {
            // Try to find the repo: cwd or current-repo marker
            let _repo_path = match kq_config::repo_path(None) {
                Ok(p) if p.join("knowledge.toml").exists() => p,
                _ => {
                    let marker =
                        dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".kq").join("current-repo");
                    if marker.exists() {
                        let path = std::fs::read_to_string(&marker).unwrap_or_default().trim().to_string();
                        PathBuf::from(path)
                    } else {
                        anyhow::bail!(
                            "Cannot determine knowledge repo. Run `kqs init` first or click the link from within a repo directory."
                        );
                    }
                }
            };

            let path = url.strip_prefix("kq://").or_else(|| url.strip_prefix("kq:")).unwrap_or(&url);
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() < 2 {
                anyhow::bail!("Invalid kq:// URL: {}", url);
            }
            match parts[0] {
                "task" | "tasks" => match parts[1] {
                    "move" if parts.len() >= 3 => {
                        let raw_status = if parts.len() > 3 { parts[3] } else { "todo" };
                        let new_status = raw_status.to_lowercase();
                        let status: kq_core::task::Status = new_status.parse()?;
                        let task = kq_core::task::task_move(parts[2], status)?;
                        println!("Moved {} to {}", task.id, task.status);
                    }
                    "assign" if parts.len() >= 4 => {
                        let task = kq_core::task::task_assign(parts[2], parts[3])?;
                        println!("Assigned {} to {}", task.id, task.assignee);
                    }
                    "show" if parts.len() >= 3 => {
                        let task = kq_core::task::task_show(parts[2])?;
                        println!("{} — {}", task.id, task.title);
                    }
                    _ => anyhow::bail!("Unknown task command in URL: {}", url),
                },
                _ => anyhow::bail!("Unknown command in kq:// URL: {}", url),
            }
        }
        Command::InstallProtocol => {
            install_protocol()?;
        }
        Command::LlmHelp => {
            print!("{}", kq_core::llm_help::generate());
        }
    }

    Ok(())
}

/// Install the `kq://` URL scheme handler on the current OS.
fn install_protocol() -> Result<()> {
    let kq_path = std::env::current_exe().context("Cannot determine kq binary path")?;

    #[cfg(target_os = "macos")]
    {
        // On macOS, register kq:// URL scheme via LaunchServices defaults
        // and create a simple handler script
        let handler_script =
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".kq").join("kq-url-handler.sh");

        // Write a handler script that receives the URL as argument
        let log_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".kq").join("url-handler.log");

        let content = format!(
            r#"#!/bin/bash
LOG="{}"
echo "[$(date)] URL=$1" >> "$LOG"
"{}" handle-url "$1" >> "$LOG" 2>&1
echo "[$(date)] exit=$?" >> "$LOG"
"#,
            log_path.display(),
            kq_path.display()
        );
        fs::write(&handler_script, &content)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&handler_script, std::fs::Permissions::from_mode(0o755))?;

        // Use Swift to create a tiny helper app that handles the get-url AppleEvent
        let swift_src = format!(
            r#"import Foundation
import AppKit

let kq = "{}"

func handleURL(_ url: String) {{
    let task = Process()
    task.launchPath = kq
    task.arguments = ["handle-url", url]
    try? task.run()
    task.waitUntilExit()
}}

// Direct invocation (e.g. from script or terminal)
if CommandLine.arguments.count > 1 {{
    handleURL(CommandLine.arguments[1])
    exit(0)
}}

// URL scheme handler — capture get-url AppleEvent
class App: NSObject, NSApplicationDelegate {{
    func application(_ application: NSApplication, open urls: [URL]) {{
        if let url = urls.first {{
            handleURL(url.absoluteString)
        }}
        exit(0)
    }}
}}

let app = NSApplication.shared
let delegate = App()
app.delegate = delegate
app.setActivationPolicy(.prohibited)
app.run()
"#,
            kq_path.display()
        );
        let swift_path = handler_script.with_extension("swift");

        fs::write(&swift_path, &swift_src)?;

        let compiled_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".kq").join("kq-url-handler");

        let status = std::process::Command::new("swiftc")
            .args(["-o", &compiled_path.to_string_lossy(), &swift_path.to_string_lossy()])
            .status()
            .context("Failed to compile Swift handler")?;

        if !status.success() {
            eprintln!("Warning: Swift compilation failed, using shell script fallback");
            // Fall back to registering the shell script
            let status = std::process::Command::new("defaults")
                .args([
                    "write",
                    "com.apple.LaunchServices/com.apple.launchservices.secure",
                    "LSHandlers",
                    "-array-add",
                    "{LSHandlerURLScheme=kq;LSHandlerRoleAll=com.kq.urlhandler;}",
                ])
                .status()?;
            if status.success() {
                println!("Registered kq:// protocol handler (shell fallback)");
                println!("NOTE: Some apps may need a restart to recognize the handler.");
            }
        } else {
            // Register the compiled binary with LaunchServices via a minimal .app
            let app_dir =
                dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("Applications").join("kq-handler.app");

            let macos_dir = app_dir.join("Contents").join("MacOS");
            fs::create_dir_all(&macos_dir)?;

            // Symlink the compiled binary into the .app bundle
            let symlink_path = macos_dir.join("kq-handler");
            if symlink_path.exists() {
                fs::remove_file(&symlink_path).ok();
            }
            std::os::unix::fs::symlink(&compiled_path, &symlink_path)?;

            let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.kq.urlhandler</string>
    <key>CFBundleName</key>
    <string>kq URL Handler</string>
    <key>CFBundleExecutable</key>
    <string>kq-handler</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>kq</string>
            </array>
        </dict>
    </array>
</dict>
</plist>"#
                .to_string();

            let plist_path = app_dir.join("Contents").join("Info.plist");
            fs::write(&plist_path, &plist)?;

            // Copy the compiled binary (not symlink — codesign needs regular file)
            let binary_path = macos_dir.join("kq-handler");
            if binary_path.exists() {
                fs::remove_file(&binary_path).ok();
            }
            std::fs::copy(&compiled_path, &binary_path).context("Failed to copy handler binary into .app")?;

            // Try to codesign (optional — URL scheme needs signing on macOS 26+)
            let _ = std::process::Command::new("codesign")
                .args(["--force", "--deep", "--sign", "-", &app_dir.to_string_lossy()])
                .status();

            let ls_status = std::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
                .args(["-f", &app_dir.to_string_lossy()])
                .status()?;

            if ls_status.success() {
                println!("Registered kq:// protocol handler at {}", app_dir.display());
                println!("Now you can use [links](kq://task/move/TASK-001/review) in .md files.");
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let reg_script = format!(
            r#"Windows Registry Editor Version 5.00

[HKEY_CLASSES_ROOT\kq]
@="URL:kq Protocol"
"URL Protocol"=""

[HKEY_CLASSES_ROOT\kq\shell]

[HKEY_CLASSES_ROOT\kq\shell\open]

[HKEY_CLASSES_ROOT\kq\shell\open\command]
@="\"{}\" handle-url \"%1\""
"#,
            kq_path.to_string_lossy().replace('\\', "\\\\")
        );

        let reg_path =
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".kq").join("install-kq-protocol.reg");

        fs::create_dir_all(reg_path.parent().unwrap())?;
        fs::write(&reg_path, &reg_script)?;
        println!("Created registration file at {}", reg_path.display());
        println!("Run the following as Administrator to register kq:// protocol:");
        println!("  regedit /s {}", reg_path.display());
    }

    #[cfg(target_os = "linux")]
    {
        let desktop = format!(
            r#"[Desktop Entry]
Type=Application
Name=kq URL Handler
Exec={} handle-url %u
StartupNotify=true
MimeType=x-scheme-handler/kq;
"#,
            kq_path.display()
        );

        let desktop_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share/applications/kq-url-handler.desktop");

        fs::create_dir_all(desktop_path.parent().unwrap())?;
        fs::write(&desktop_path, &desktop)?;
        std::process::Command::new("xdg-mime")
            .args(["default", "kq-url-handler.desktop", "x-scheme-handler/kq"])
            .status()
            .context("Failed to register with xdg-mime")?;
        println!("Registered kq:// protocol handler");
    }

    Ok(())
}
/// Walk up from the current directory looking for `knowledge.toml`.
///
/// Returns `KnowledgeConfig::load()` if found, otherwise a default config.
fn load_config_for_watch() -> Result<kq_config::KnowledgeConfig> {
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd.as_path());
        while let Some(d) = dir {
            let config_path = d.join("knowledge.toml");
            if config_path.exists() {
                return kq_config::KnowledgeConfig::load(&config_path);
            }
            dir = d.parent();
        }
    }
    Ok(kq_config::KnowledgeConfig::default())
}

/// Resolve the repo path: explicit --path flag, or auto-detect via kq_config.
fn resolve_repo_path(path: Option<&std::path::Path>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(expand_tilde(p)),
        None => kq_config::repo_path(None),
    }
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix('~')
        && let Some(home) = dirs::home_dir()
    {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return if rest.is_empty() { home } else { home.join(rest) };
    }
    path.to_path_buf()
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

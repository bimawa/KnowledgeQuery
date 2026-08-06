use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeModel {
    pub name: String,
    pub file: String,
    pub doc_refs: Vec<String>,
}

pub fn new_type(path: &Path, name: &str) -> Result<String> {
    let ts_dir = path.join("TypeSpec");
    fs::create_dir_all(&ts_dir)
        .with_context(|| format!("Failed to create TypeSpec directory at {}", ts_dir.display()))?;

    let file_name = format!("{name}.tsp");
    let file_path = ts_dir.join(&file_name);

    if file_path.exists() {
        anyhow::bail!("TypeSpec file already exists: {}", file_path.display());
    }

    let model_name = capitalize_first(name);

    let content = format!(
        r#"import "@typespec/http";

namespace KQ.Data;

model {model_name} {{
  // @doc TZ-001
  // TODO: add fields
}}

"#,
    );

    fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write TypeSpec file {}", file_path.display()))?;

    Ok(file_path.display().to_string())
}

pub fn list_types(path: &Path) -> Result<Vec<TypeModel>> {
    let ts_dir = path.join("TypeSpec");

    if !ts_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut models = Vec::new();

    for entry in fs::read_dir(&ts_dir).context("Failed to read TypeSpec directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "tsp") {
            let content = fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            let doc_refs = extract_doc_refs(&content);

            for line in content.lines() {
                if let Some(name) = extract_model_name(line) {
                    models.push(TypeModel { name, file: file_name.clone(), doc_refs: doc_refs.clone() });
                }
            }
        }
    }

    Ok(models)
}

pub fn init_main_tsp(path: &Path) -> Result<()> {
    let ts_dir = path.join("TypeSpec");
    fs::create_dir_all(&ts_dir)
        .with_context(|| format!("Failed to create TypeSpec directory at {}", ts_dir.display()))?;

    let main_path = ts_dir.join("main.tsp");

    let content = if main_path.exists() {
        let existing =
            fs::read_to_string(&main_path).with_context(|| format!("Failed to read {}", main_path.display()))?;

        let imports = collect_imports(&ts_dir)?;
        let import_lines: Vec<String> = imports.iter().map(|i| format!("import \"{i}\";")).collect();
        let import_block = import_lines.join("\n");

        let mut result = format!("{}\n", import_block);
        for line in existing.lines() {
            if !line.trim_start().starts_with("import \"") {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    } else {
        let imports = collect_imports(&ts_dir)?;
        let import_lines: Vec<String> = imports.iter().map(|i| format!("import \"{i}\";")).collect();

        let mut content = import_lines.join("\n");
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str("namespace KQ.Data;\n");

        content
    };

    fs::write(&main_path, &content).with_context(|| format!("Failed to write {}", main_path.display()))?;

    Ok(())
}

fn collect_imports(ts_dir: &Path) -> Result<Vec<String>> {
    let mut imports = Vec::new();

    for entry in fs::read_dir(ts_dir).context("Failed to read TypeSpec directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "tsp")
            && path.file_name() != Some(std::ffi::OsStr::new("main.tsp"))
        {
            let content = fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;

            for line in content.lines() {
                if let Some(imp) = extract_import(line)
                    && !imports.contains(&imp)
                {
                    imports.push(imp);
                }
            }
        }
    }

    imports.sort();
    Ok(imports)
}

fn extract_model_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("model ") {
        let name: String = rest.chars().take_while(|c| *c != ' ' && *c != '{' && *c != '\n').collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn extract_doc_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(after_comment) = trimmed.strip_prefix("//") {
            let after_comment = after_comment.trim();
            if let Some(after_doc) = after_comment.strip_prefix("@doc ") {
                let ref_str: String = after_doc.chars().take_while(|c| !c.is_whitespace()).collect();
                if !ref_str.is_empty() {
                    refs.push(ref_str);
                }
            }
        }
    }
    refs
}

fn extract_import(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("import \"")
        && let Some(end) = rest.find('"')
    {
        return Some(rest[..end].to_string());
    }
    None
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_temp_dir() -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("kq_typespec_test_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_new_type_creates_file() {
        let dir = unique_temp_dir();
        let path = new_type(&dir, "person").unwrap();

        assert!(std::path::Path::new(&path).exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("model Person {"));
        assert!(content.contains("// @doc TZ-001"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_new_type_fails_if_exists() {
        let dir = unique_temp_dir();
        let _ = new_type(&dir, "person");
        assert!(new_type(&dir, "person").is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_new_type_creates_typespec_dir() {
        let dir = unique_temp_dir();
        assert!(!dir.join("TypeSpec").exists());
        let _ = new_type(&dir, "item").unwrap();
        assert!(dir.join("TypeSpec").is_dir());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_types_empty() {
        let dir = unique_temp_dir();
        let models = list_types(&dir).unwrap();
        assert!(models.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_types_finds_model() {
        let dir = unique_temp_dir();
        let _ = new_type(&dir, "order").unwrap();
        let models = list_types(&dir).unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "Order");
        assert_eq!(models[0].file, "order.tsp");
        assert!(models[0].doc_refs.contains(&"TZ-001".to_string()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_types_multiple_files() {
        let dir = unique_temp_dir();
        let _ = new_type(&dir, "alpha").unwrap();
        let _ = new_type(&dir, "beta").unwrap();
        let models = list_types(&dir).unwrap();

        assert_eq!(models.len(), 2);
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Beta"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_init_main_tsp_creates_file() {
        let dir = unique_temp_dir();
        init_main_tsp(&dir).unwrap();

        let main_path = dir.join("TypeSpec/main.tsp");
        assert!(main_path.exists());
        let content = fs::read_to_string(&main_path).unwrap();
        assert!(content.contains("namespace KQ.Data;"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_init_main_tsp_collects_imports() {
        let dir = unique_temp_dir();
        let _ = new_type(&dir, "widget").unwrap();
        init_main_tsp(&dir).unwrap();

        let content = fs::read_to_string(dir.join("TypeSpec/main.tsp")).unwrap();
        assert!(content.contains("import \"@typespec/http\";"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("A"), "A");
        assert_eq!(capitalize_first("already"), "Already");
    }

    #[test]
    fn test_extract_model_name() {
        assert_eq!(extract_model_name("model Foo {"), Some("Foo".into()));
        assert_eq!(extract_model_name("  model Bar {"), Some("Bar".into()));
        assert_eq!(extract_model_name("// model Baz {"), None);
        assert_eq!(extract_model_name("model {"), None);
    }

    #[test]
    fn test_extract_doc_refs() {
        let content = "// @doc TZ-100\nmodel X {}\n// @doc TZ-200\nmodel Y {}";
        let refs = extract_doc_refs(content);
        assert_eq!(refs, vec!["TZ-100".to_string(), "TZ-200".to_string()]);
    }

    #[test]
    fn test_extract_import() {
        assert_eq!(extract_import("import \"@typespec/http\";"), Some("@typespec/http".into()));
        assert_eq!(extract_import("// not an import"), None);
        assert_eq!(extract_import("namespace Foo;"), None);
    }

    #[test]
    fn test_list_types_ignores_non_tsp_files() {
        let dir = unique_temp_dir();
        let ts_dir = dir.join("TypeSpec");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(ts_dir.join("readme.md"), "not a tsp file").unwrap();

        let models = list_types(&dir).unwrap();
        assert!(models.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}

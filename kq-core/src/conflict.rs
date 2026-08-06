use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub fn list(path: &Path) -> Result<Vec<String>> {
    let repo = crate::git::open_repo(path)?;
    let index = repo.index().context("Failed to open index")?;
    let mut conflicts = Vec::new();
    for conflict in index.conflicts().context("Failed to read conflicts")? {
        let conflict = conflict.context("Failed to read conflict entry")?;
        if let Some(our) = &conflict.our
            && let Ok(name) = std::str::from_utf8(&our.path)
        {
            conflicts.push(name.to_owned());
        }
    }
    Ok(conflicts)
}

pub fn show(path: &Path, file: &str) -> Result<String> {
    let content =
        fs::read_to_string(path.join(file)).with_context(|| format!("Failed to read conflicted file: {}", file))?;
    Ok(content)
}

pub fn resolve_ours(path: &Path, file: &str) -> Result<()> {
    let repo = crate::git::open_repo(path)?;
    let index = repo.index().context("Failed to open index")?;
    let all: Vec<_> = index.conflicts().context("Failed to read conflicts")?.filter_map(|c| c.ok()).collect();
    drop(index);
    for conflict in &all {
        if let Some(our) = &conflict.our
            && let Ok(name) = std::str::from_utf8(&our.path)
            && name == file
        {
            let mut index = repo.index().context("Failed to open index")?;
            index.add(our).context("Failed to stage OURS version")?;
            index.remove(Path::new(file), 0).context("Failed to remove conflict marker")?;
            index.write().context("Failed to write index")?;
            return Ok(());
        }
    }
    anyhow::bail!("No conflict found for file: {}", file);
}

pub fn resolve_theirs(path: &Path, file: &str) -> Result<()> {
    let repo = crate::git::open_repo(path)?;
    let index = repo.index().context("Failed to open index")?;
    let all: Vec<_> = index.conflicts().context("Failed to read conflicts")?.filter_map(|c| c.ok()).collect();
    drop(index);
    for conflict in &all {
        if let Some(their) = &conflict.their
            && let Ok(name) = std::str::from_utf8(&their.path)
            && name == file
        {
            let mut index = repo.index().context("Failed to open index")?;
            index.add(their).context("Failed to stage THEIRS version")?;
            index.remove(Path::new(file), 0).context("Failed to remove conflict marker")?;
            index.write().context("Failed to write index")?;
            return Ok(());
        }
    }
    anyhow::bail!("No conflict found for file: {}", file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_no_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = list(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn show_missing_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = show(dir.path(), "nonexistent.md");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_ours_no_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_ours(dir.path(), "file.md");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_theirs_no_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_theirs(dir.path(), "file.md");
        assert!(result.is_err());
    }
}

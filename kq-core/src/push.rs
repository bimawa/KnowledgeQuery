use std::path::Path;

use anyhow::{Context, Result};
use git2::{Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, Signature};

use crate::git;

fn current_branch_name(repo: &Repository) -> Result<String> {
    let head = repo.head().context("Failed to read HEAD")?;
    let branch = head.shorthand().context("HEAD is not a branch")?.to_string();
    Ok(branch)
}

fn amend_commit_with_readme(repo: &Repository) -> Result<git2::Oid> {
    let mut index = repo.index().context("Failed to open index")?;
    index.add_path(Path::new("README.md")).context("Failed to stage README.md")?;
    index.write().context("Failed to write index")?;

    let tree_oid = index.write_tree().context("Failed to write tree")?;
    let tree = repo.find_tree(tree_oid).context("Failed to find tree")?;

    let head_commit = repo.head().context("No HEAD")?.peel_to_commit().context("HEAD not a commit")?;

    let signature = Signature::now("kq", "kq@knowledge").context("Failed to create signature")?;
    let message = format!("docs: update README [{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));

    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, &message, &tree, &[&head_commit])
        .context("Failed to amend commit")?;

    Ok(oid)
}

fn commit_without_readme(repo: &Repository) -> Result<Option<git2::Oid>> {
    let mut index = repo.index().context("Failed to open index")?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).context("Failed to stage files")?;
    index.write().context("Failed to write index")?;

    let tree_oid = index.write_tree().context("Failed to write tree")?;
    let tree = repo.find_tree(tree_oid).context("Failed to find tree")?;

    if let Ok(head) = repo.head()
        && let Ok(head_commit) = head.peel_to_commit()
        && let Ok(head_tree) = head_commit.tree()
        && head_tree.id() == tree_oid
    {
        return Ok(None);
    }

    let signature = Signature::now("kq", "kq@knowledge").context("Failed to create signature")?;
    let message = format!("docs: auto-sync [{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));

    let oid = match repo.head() {
        Ok(head_ref) => {
            let parent = head_ref.peel_to_commit().context("Failed to get HEAD commit")?;
            repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[&parent])
        }
        Err(_) => repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[]),
    }
    .context("Failed to create commit")?;

    Ok(Some(oid))
}

pub fn push(path: &Path, dry_run: bool, no_readme: bool) -> Result<()> {
    let repo = git::open_repo(path)?;
    let branch = current_branch_name(&repo)?;
    let remote_name = "origin";
    let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);

    let mut remote = repo.find_remote(remote_name).with_context(|| format!("Remote '{}' not found", remote_name))?;

    let mut fetch_callbacks = RemoteCallbacks::new();
    fetch_callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::USERNAME) {
            return Cred::username(username_from_url.unwrap_or("git"));
        }
        Cred::default()
    });
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(fetch_callbacks);
    let fetch_result = remote.fetch(&[&branch], Some(&mut fetch_options), None);

    if let Err(e) = fetch_result {
        if e.class() == git2::ErrorClass::Rebase || e.code() == git2::ErrorCode::Conflict {
            return Err(e).context("kq conflict: rebase produced conflicts — resolve manually");
        }
        return Err(e).context("Failed to fetch from remote");
    }

    let remote_branch_ref = format!("remotes/{}/{}", remote_name, branch);
    let remote_oid = repo
        .refname_to_id(&remote_branch_ref)
        .with_context(|| format!("Remote branch '{}' not found", remote_branch_ref))?;
    let head_oid = repo.head().context("No HEAD")?.target().context("HEAD has no target")?;

    if head_oid == remote_oid {
        eprintln!("[kq] Already up-to-date with {}/{}", remote_name, branch);
        return Ok(());
    }

    if dry_run {
        let head_commit = repo.head().context("No HEAD")?.peel_to_commit().context("HEAD not a commit")?;
        let remote_commit = repo.find_commit(remote_oid)?;

        eprintln!("[kq] Dry run — would push:");
        eprintln!("  local:  {} {}", &head_commit.id().to_string()[..8], head_commit.summary().unwrap_or(""));
        eprintln!("  remote: {} {}", &remote_commit.id().to_string()[..8], remote_commit.summary().unwrap_or(""));
        return Ok(());
    }

    let oid = if !no_readme {
        eprintln!("[kq] Generating README...");
        crate::readme_gen::generate(path)?;
        amend_commit_with_readme(&repo)?
    } else {
        match commit_without_readme(&repo)? {
            Some(oid) => oid,
            None => {
                eprintln!("[kq] No changes to push");
                return Ok(());
            }
        }
    };

    let mut push_callbacks = RemoteCallbacks::new();
    push_callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::USERNAME) {
            return Cred::username(username_from_url.unwrap_or("git"));
        }
        Cred::default()
    });

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(push_callbacks);

    remote.push(&[&refspec], Some(&mut push_opts)).context("Failed to push to remote")?;

    eprintln!("[kq] Pushed {} to {}", &oid.to_string()[..8], remote_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_with_commit(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        let sig = Signature::now("test", "test@test").unwrap();
        let mut index = repo.index().unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        drop(tree);
        repo
    }

    #[test]
    fn test_current_branch_name() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        let name = current_branch_name(&repo).unwrap();
        assert!(!name.is_empty(), "branch name should not be empty");
    }

    #[test]
    fn test_commit_without_readme() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        fs::write(dir.path().join("note.md"), "# Note\n").unwrap();
        let result = commit_without_readme(&repo).unwrap();
        assert!(result.is_some(), "should create commit with changes");
    }

    #[test]
    fn test_commit_without_readme_no_changes() {
        let dir = TempDir::new().unwrap();
        let repo = init_with_commit(dir.path());
        let result = commit_without_readme(&repo).unwrap();
        assert!(result.is_none(), "should return None with no changes");
    }
}

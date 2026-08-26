use std::path::{Component, Path, PathBuf};

use git2::{ObjectType, Oid, Repository, Tree};
use thiserror::Error;
use url::Url;

#[cfg(test)]
mod tests {
    use super::validate_remote_url_scheme;

    #[test]
    fn rejects_a_file_url() {
        assert!(validate_remote_url_scheme("file:///etc/passwd").is_err());
    }

    #[test]
    fn allows_https_ssh_and_git_urls() {
        assert!(validate_remote_url_scheme("https://example.test/repo.git").is_ok());
        assert!(validate_remote_url_scheme("ssh://git@example.test/repo.git").is_ok());
        assert!(validate_remote_url_scheme("git://example.test/repo.git").is_ok());
    }

    #[test]
    fn allows_scp_shorthand_and_bare_local_paths() {
        assert!(validate_remote_url_scheme("git@example.test:org/repo.git").is_ok());
        assert!(validate_remote_url_scheme("../sibling-repo").is_ok());
        assert!(validate_remote_url_scheme("/abs/path/to/repo").is_ok());
    }
}

#[derive(Debug, Error)]
pub enum SubmoduleError {
    #[error("submodule was not found")]
    NotFound,
    #[error("submodule path is invalid")]
    InvalidPath,
    #[error("submodule url uses a disallowed scheme: {0}")]
    DisallowedUrlScheme(String),
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleInfo {
    pub path: String,
    pub url: Option<String>,
    pub gitlink_id: Option<String>,
    pub initialized: bool,
    pub head_id: Option<String>,
}

pub fn list_submodules(repo: &Repository) -> Result<Vec<SubmoduleInfo>, SubmoduleError> {
    let config = repo.config()?;
    repo.submodules()?
        .into_iter()
        .map(|submodule| {
            let name = submodule.name()?;
            Ok(SubmoduleInfo {
                path: submodule.path().to_string_lossy().into_owned(),
                url: submodule.url()?.map(str::to_string),
                gitlink_id: submodule.head_id().map(|id| id.to_string()),
                initialized: config.get_string(&format!("submodule.{name}.url")).is_ok(),
                head_id: submodule.workdir_id().map(|id| id.to_string()),
            })
        })
        .collect()
}

pub fn init_submodule(repo: &Repository, path: &str) -> Result<(), SubmoduleError> {
    find_submodule(repo, path)?.init(false)?;
    Ok(())
}

pub fn update_submodule(
    repo: &Repository,
    path: &str,
    recursive: bool,
) -> Result<(), SubmoduleError> {
    let mut submodule = find_submodule(repo, path)?;

    // Validated unconditionally, not just on the recursive path: `submodule.update` below fetches
    // straight from this configured URL via libgit2's own native update when the submodule isn't
    // yet checked out, with no protocol check of its own — the recursive-only preflight
    // (`ensure_recursive_update_is_safe`) doesn't run for a plain non-recursive update, which was
    // the actual common case (the frontend defaults to non-recursive) SEC-002 initially missed.
    // Reads the local `submodule.<name>.url` config key, same as `ensure_recursive_update_is_safe`
    // does below, since that (not `Submodule::url()`, which reflects `.gitmodules` and ignores a
    // local override) is what `submodule.update` actually fetches from once initialized.
    if let Ok(name) = submodule.name() {
        if let Ok(url) = repo.config()?.get_string(&format!("submodule.{name}.url")) {
            validate_remote_url_scheme(&url)?;
        }
    }

    if recursive {
        ensure_recursive_update_is_safe(repo, &submodule)?;
    }

    submodule.update(false, None)?;

    if recursive {
        update_nested_submodules(&submodule.open()?)?;
    }

    Ok(())
}

fn find_submodule<'repo>(
    repo: &'repo Repository,
    path: &str,
) -> Result<git2::Submodule<'repo>, SubmoduleError> {
    validate_path(path)?;
    repo.submodules()?
        .into_iter()
        .find(|submodule| submodule.path().to_str() == Some(path))
        .ok_or(SubmoduleError::NotFound)
}

fn update_nested_submodules(repo: &Repository) -> Result<(), SubmoduleError> {
    for mut submodule in repo.submodules()? {
        submodule.update(false, None)?;
        update_nested_submodules(&submodule.open()?)?;
    }
    Ok(())
}

fn ensure_recursive_update_is_safe(
    repo: &Repository,
    submodule: &git2::Submodule<'_>,
) -> Result<(), SubmoduleError> {
    let target_id = submodule
        .index_id()
        .ok_or_else(|| git2::Error::from_str("submodule has no index entry"))?;
    let name = submodule.name()?;
    let url = repo
        .config()?
        .get_string(&format!("submodule.{name}.url"))
        .map_err(|_| git2::Error::from_str("submodule is not initialized"))?;

    if submodule.workdir_id().is_some() {
        let selected_repo = submodule.open()?;
        download_target_from_origin(&selected_repo, target_id)?;
        ensure_nested_submodules_initialized_at(&selected_repo, target_id, Path::new(""))?;
    } else {
        download_target_from_url(repo, &url, target_id)?;
        reject_nested_gitlinks(repo, target_id, Path::new(""))?;
    }

    Ok(())
}

fn ensure_nested_submodules_initialized_at(
    repo: &Repository,
    target_id: Oid,
    prefix: &Path,
) -> Result<(), SubmoduleError> {
    let config = repo.config()?;
    let submodules = repo.submodules()?;

    for (path, nested_target_id) in target_gitlinks(repo, target_id)? {
        let full_path = prefix.join(&path);
        let Some(submodule) = submodules.iter().find(|submodule| submodule.path() == path) else {
            return Err(uninitialized_nested_submodule(&full_path));
        };

        let name = submodule.name()?;
        let url = config
            .get_string(&format!("submodule.{name}.url"))
            .map_err(|_| uninitialized_nested_submodule(&full_path))?;

        if submodule.workdir_id().is_some() {
            let nested_repo = submodule.open()?;
            download_target_from_origin(&nested_repo, nested_target_id)?;
            ensure_nested_submodules_initialized_at(&nested_repo, nested_target_id, &full_path)?;
        } else {
            download_target_from_url(repo, &url, nested_target_id)?;
            reject_nested_gitlinks(repo, nested_target_id, &full_path)?;
        }
    }

    Ok(())
}

fn reject_nested_gitlinks(
    repo: &Repository,
    target_id: Oid,
    prefix: &Path,
) -> Result<(), SubmoduleError> {
    if let Some((path, _)) = target_gitlinks(repo, target_id)?.into_iter().next() {
        return Err(uninitialized_nested_submodule(&prefix.join(path)));
    }
    Ok(())
}

fn target_gitlinks(
    repo: &Repository,
    target_id: Oid,
) -> Result<Vec<(PathBuf, Oid)>, SubmoduleError> {
    let tree = repo.find_commit(target_id)?.tree()?;
    let mut gitlinks = Vec::new();
    collect_gitlinks(repo, &tree, Path::new(""), &mut gitlinks)?;
    Ok(gitlinks)
}

fn collect_gitlinks(
    repo: &Repository,
    tree: &Tree<'_>,
    prefix: &Path,
    gitlinks: &mut Vec<(PathBuf, Oid)>,
) -> Result<(), SubmoduleError> {
    for entry in tree.iter() {
        let name = entry.name()?;
        let path = prefix.join(name);
        match entry.kind() {
            Some(ObjectType::Commit) => gitlinks.push((path, entry.id())),
            Some(ObjectType::Tree) => {
                collect_gitlinks(repo, &repo.find_tree(entry.id())?, &path, gitlinks)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn download_target_from_origin(repo: &Repository, target_id: Oid) -> Result<(), SubmoduleError> {
    if repo.find_commit(target_id).is_err() {
        let mut origin = repo.find_remote("origin")?;
        origin.download::<&str>(&[], None)?;
    }
    repo.find_commit(target_id)?;
    Ok(())
}

fn download_target_from_url(
    repo: &Repository,
    url: &str,
    target_id: Oid,
) -> Result<(), SubmoduleError> {
    if repo.find_commit(target_id).is_err() {
        validate_remote_url_scheme(url)?;
        let mut remote = repo.remote_anonymous(url)?;
        remote.download(&["+refs/heads/*:refs/remotes/browsitory-preflight/*"], None)?;
    }
    repo.find_commit(target_id)?;
    Ok(())
}

/// Rejects an explicit `file://` submodule URL before this preflight check fetches it. `url`
/// here comes straight from `.gitmodules`/`submodule.<name>.url`, i.e. it's attacker-controlled
/// if the outer repo is untrusted — a malicious `.gitmodules` pointing a submodule at
/// `file:///etc/passwd`-style would otherwise let this preflight check read arbitrary local
/// paths on the host. See issue SEC-002.
///
/// Deliberately narrow: bare/relative local filesystem paths (`../sibling-repo`) and scp-like
/// `user@host:path` SSH shorthand are left alone, matching vanilla `git submodule`'s own
/// behavior — this repo's own test fixtures rely on local-path "remotes" to simulate a remote
/// without a network, and a real user may legitimately point a submodule at a local repo. Only
/// an explicit `file://` scheme is both a distinct attacker-actionable vector (real
/// `.gitmodules` configs targeting a local repo essentially never write it with a `file://`
/// prefix) and reliably detectable via `Url::parse`, unlike distinguishing a hostile bare path
/// from a legitimate one.
fn validate_remote_url_scheme(url: &str) -> Result<(), SubmoduleError> {
    if let Ok(parsed) = Url::parse(url) {
        if parsed.scheme() == "file" {
            return Err(SubmoduleError::DisallowedUrlScheme(url.to_string()));
        }
    }
    Ok(())
}

fn uninitialized_nested_submodule(path: &Path) -> SubmoduleError {
    let display_path = path.to_string_lossy().replace("\\", "/");
    git2::Error::from_str(&format!(
        "nested submodule is not initialized: {}",
        display_path
    ))
    .into()
}

fn validate_path(path: &str) -> Result<(), SubmoduleError> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SubmoduleError::InvalidPath);
    }
    Ok(())
}

use std::path::{Component, Path, PathBuf};

use git2::{ObjectType, Oid, Repository, Tree};
use thiserror::Error;
use url::Url;

use crate::worktree_guard::{CasePolicy, GuardSource};

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
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
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
    update_submodule_with(repo, path, recursive, CasePolicy::FromRepo)
}

/// [`update_submodule`] with the case-collision policy as a parameter (tests force it on Linux).
pub(crate) fn update_submodule_with(
    repo: &Repository,
    path: &str,
    recursive: bool,
    policy: CasePolicy,
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

    // libgit2 clones into the submodule's path as it finds it, following a symlink that stands
    // there or above it out of the repository.
    if let Some(workdir) = repo.workdir() {
        crate::worktree_guard::refuse_linked_path(workdir, submodule.path())?;
    }

    if recursive {
        ensure_recursive_update_is_safe(repo, &submodule)?;
    }

    guarded_update(repo, &mut submodule, policy)?;

    if recursive {
        update_nested_submodules(&submodule.open()?, policy)?;
    }

    Ok(())
}

/// `submodule.update`, but the commit's tree is checked for case collisions (#156) and for links
/// standing where it has folders (#114) before anything is written in the submodule's work tree.
///
/// A checked-out submodule is checked against its target commit, then updated by libgit2 as
/// before. A fresh one is cloned without a checkout (libgit2 sets its HEAD to the target), then
/// checked, then checked out (`checkout_head`, safe, recreating the missing files). A refused fresh
/// submodule stays cloned with an empty work tree.
fn guarded_update(
    repo: &Repository,
    submodule: &mut git2::Submodule<'_>,
    policy: CasePolicy,
) -> Result<(), SubmoduleError> {
    let name = submodule.name()?.to_string();
    let status = repo.submodule_status(&name, git2::SubmoduleIgnore::None)?;
    // libgit2 clones only into an empty folder; anything else it opens as a checked-out
    // submodule (and fails as it would, when that isn't one).
    if !status.contains(git2::SubmoduleStatus::WD_UNINITIALIZED) {
        let sub = submodule.open()?;
        let target = submodule
            .index_id()
            .ok_or_else(|| git2::Error::from_str("could not get ID of submodule in index"))?;
        if sub.find_commit(target).is_err() {
            // libgit2 would fetch it from the submodule's default remote too.
            download_target_from_origin(&sub, target)?;
        }
        let commit = sub.find_commit(target)?;
        let tree = commit.tree()?;
        check_submodule_tree(&sub, &tree, policy)?;
        if workdir_is_empty(&sub) {
            // A clone whose checkout was refused (or never ran): its index already lists every
            // file, so libgit2's safe checkout would take the missing files for deletions.
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.safe().recreate_missing(true);
            sub.checkout_tree(commit.as_object(), Some(&mut checkout))?;
            sub.set_head_detached(target)?;
        } else {
            submodule.update(false, None)?;
        }
        return Ok(());
    }
    let mut no_checkout = git2::build::CheckoutBuilder::new();
    no_checkout.dry_run();
    let mut options = git2::SubmoduleUpdateOptions::new();
    options.checkout(no_checkout);
    submodule.update(false, Some(&mut options))?;
    let sub = submodule.open()?;
    let tree = sub.head()?.peel_to_tree()?;
    check_submodule_tree(&sub, &tree, policy)?;
    // The no-checkout pass already wrote HEAD's entries into the index, so the missing files
    // must be recreated explicitly.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe().recreate_missing(true);
    sub.checkout_head(Some(&mut checkout))?;
    Ok(())
}

/// True when the submodule's work tree holds nothing but its `.git`.
fn workdir_is_empty(sub: &Repository) -> bool {
    sub.workdir()
        .and_then(|workdir| std::fs::read_dir(workdir).ok())
        .is_some_and(|entries| entries.flatten().all(|entry| entry.file_name() == ".git"))
}

fn check_submodule_tree(
    sub: &Repository,
    tree: &Tree<'_>,
    policy: CasePolicy,
) -> Result<(), SubmoduleError> {
    if policy.applies(sub) {
        const ALL: Option<&[&str]> = None;
        if let Some(collision) =
            crate::worktree_guard::find_case_collision(&GuardSource::Tree(tree), ALL)?
        {
            return Err(collision.into());
        }
    }
    crate::worktree_guard::refuse_linked_folders::<SubmoduleError>(
        sub,
        &[crate::worktree_guard::Incoming::Checkout(tree)],
    )
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

fn update_nested_submodules(repo: &Repository, policy: CasePolicy) -> Result<(), SubmoduleError> {
    for mut submodule in repo.submodules()? {
        if let Some(workdir) = repo.workdir() {
            crate::worktree_guard::refuse_linked_path(workdir, submodule.path())?;
        }
        guarded_update(repo, &mut submodule, policy)?;
        update_nested_submodules(&submodule.open()?, policy)?;
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

#[cfg(test)]
mod case_tests {
    use super::*;
    use tempfile::TempDir;

    fn sig() -> git2::Signature<'static> {
        git2::Signature::now("T", "t@example.com").unwrap()
    }

    /// The submodule's source: `clean` (`a.txt`), `clean2` (`a.txt` changed, `b.txt`) and
    /// `evil` (clean + `D -> /x` + `d/x`).
    struct Source {
        dir: TempDir,
        clean: Oid,
        clean2: Oid,
        evil: Oid,
    }

    fn source() -> Source {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = |bytes: &[u8]| repo.blob(bytes).unwrap();
        let commit = |tree: Oid, parent: Option<Oid>| {
            let tree = repo.find_tree(tree).unwrap();
            let parents: Vec<_> = parent
                .map(|id| repo.find_commit(id).unwrap())
                .into_iter()
                .collect();
            let parents: Vec<_> = parents.iter().collect();
            repo.commit(None, &sig(), &sig(), "c", &tree, &parents)
                .unwrap()
        };
        let mut root = repo.treebuilder(None).unwrap();
        root.insert("a.txt", blob(b"a\n"), 0o100644).unwrap();
        let clean = commit(root.write().unwrap(), None);
        root.insert("a.txt", blob(b"a2\n"), 0o100644).unwrap();
        root.insert("b.txt", blob(b"b\n"), 0o100644).unwrap();
        let clean2 = commit(root.write().unwrap(), Some(clean));
        let mut root = repo.treebuilder(None).unwrap();
        root.insert("a.txt", blob(b"a\n"), 0o100644).unwrap();
        let mut inner = repo.treebuilder(None).unwrap();
        inner.insert("x", blob(b"PWNED\n"), 0o100644).unwrap();
        root.insert("d", inner.write().unwrap(), 0o040000).unwrap();
        root.insert("D", blob(b"/x"), 0o120000).unwrap();
        let evil = commit(root.write().unwrap(), Some(clean));
        repo.reference("refs/heads/main", clean2, true, "").unwrap();
        repo.reference("refs/heads/evil", evil, true, "").unwrap();
        repo.set_head("refs/heads/main").unwrap();
        Source {
            dir,
            clean,
            clean2,
            evil,
        }
    }

    /// A super-repository whose submodule `sub` points at `src` with its gitlink at `at`,
    /// initialized (not cloned).
    fn super_repo(src: &Source, at: Oid) -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let url = src.dir.path().to_string_lossy().replace('\\', "/");
        std::fs::write(
            dir.path().join(".gitmodules"),
            format!("[submodule \"sub\"]\n\tpath = sub\n\turl = {url}\n"),
        )
        .unwrap();
        set_gitlink(&repo, at);
        // A checkout of the super-repository leaves the submodule's folder empty.
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        init_submodule(&repo, "sub").unwrap();
        (dir, repo)
    }

    /// Commits `.gitmodules` + the gitlink `sub` at `at` (index and HEAD).
    fn set_gitlink(repo: &Repository, at: Oid) {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(".gitmodules")).unwrap();
        index
            .add(&crate::undo::index_entry("sub", at, 0o160000))
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<_> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig(), &sig(), "gitlink", &tree, &parents)
            .unwrap();
    }

    fn update(repo: &Repository) -> Result<(), SubmoduleError> {
        update_submodule_with(repo, "sub", false, CasePolicy::Always)
    }

    #[test]
    fn updating_a_submodule_whose_commit_has_a_collision_is_refused() {
        let src = source();
        let (dir, repo) = super_repo(&src, src.evil);
        let error = update(&repo).unwrap_err();
        assert!(matches!(error, SubmoduleError::CaseCollision(_)), "{error}");
        let sub = dir.path().join("sub");
        assert!(std::fs::symlink_metadata(sub.join("d")).is_err());
        assert!(std::fs::symlink_metadata(sub.join("D")).is_err());
        assert!(!sub.join("a.txt").exists());
    }

    #[test]
    fn moving_a_checked_out_submodule_to_a_collision_is_refused() {
        let src = source();
        let (dir, repo) = super_repo(&src, src.clean);
        update(&repo).unwrap();
        set_gitlink(&repo, src.evil);
        let error = update(&repo).unwrap_err();
        assert!(matches!(error, SubmoduleError::CaseCollision(_)), "{error}");
        let sub = Repository::open(dir.path().join("sub")).unwrap();
        assert_eq!(sub.head().unwrap().target(), Some(src.clean));
        assert!(std::fs::symlink_metadata(dir.path().join("sub/d")).is_err());
    }

    #[test]
    fn a_refused_fresh_submodule_is_complete_after_moving_to_a_clean_commit() {
        let src = source();
        let (dir, repo) = super_repo(&src, src.evil);
        assert!(update(&repo).is_err());
        set_gitlink(&repo, src.clean);
        update(&repo).unwrap();
        let sub_dir = dir.path().join("sub");
        assert_eq!(
            std::fs::read_to_string(sub_dir.join("a.txt")).unwrap(),
            "a\n"
        );
        let sub = Repository::open(&sub_dir).unwrap();
        assert_eq!(sub.head().unwrap().target(), Some(src.clean));
        assert!(crate::status::status(&sub).unwrap().is_empty());
    }

    #[test]
    fn updating_a_clean_submodule_populates_it_and_moves_it() {
        let src = source();
        let (dir, repo) = super_repo(&src, src.clean);
        update(&repo).unwrap();
        let sub_dir = dir.path().join("sub");
        assert_eq!(
            std::fs::read_to_string(sub_dir.join("a.txt")).unwrap(),
            "a\n"
        );
        let sub = Repository::open(&sub_dir).unwrap();
        assert!(crate::status::status(&sub).unwrap().is_empty());

        set_gitlink(&repo, src.clean2);
        update(&repo).unwrap();
        assert_eq!(
            std::fs::read_to_string(sub_dir.join("a.txt")).unwrap(),
            "a2\n"
        );
        assert_eq!(
            std::fs::read_to_string(sub_dir.join("b.txt")).unwrap(),
            "b\n"
        );
        assert_eq!(sub.head().unwrap().target(), Some(src.clean2));
        assert!(crate::status::status(&sub).unwrap().is_empty());
    }
}

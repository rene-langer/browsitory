use git2::Repository;

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
    pub push_url: Option<String>,
}

/// Lists configured remotes. Skips entries libgit2 can't give a valid UTF-8
/// name for (`StringArray::iter()` yields `Result<Option<&str>, Error>` per
/// slot — `Err` on a lookup failure, `Ok(None)` for a non-UTF8 name; the
/// double `.flatten()` below drops both) — same defensive-skip pattern as
/// `branch::list_branches`.
pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>> {
    let mut out = Vec::new();
    for name in repo.remotes()?.iter().flatten().flatten() {
        let Ok(remote) = repo.find_remote(name) else {
            continue;
        };
        // `Remote::url()` is `Result<&str, Error>`, `pushurl()` is
        // `Result<Option<&str>, Error>` — same fallible-accessor footgun
        // class documented in CLAUDE.md for `shorthand()`/`summary()`.
        let Ok(url) = remote.url() else {
            continue;
        };
        let push_url = remote.pushurl().ok().flatten().map(str::to_string);
        out.push(RemoteInfo {
            name: name.to_string(),
            url: url.to_string(),
            push_url,
        });
    }
    Ok(out)
}

pub fn add_remote(repo: &Repository, name: &str, url: &str) -> Result<()> {
    repo.remote(name, url)?;
    Ok(())
}

pub fn remove_remote(repo: &Repository, name: &str) -> Result<()> {
    repo.remote_delete(name)?;
    Ok(())
}

pub fn rename_remote(repo: &Repository, old_name: &str, new_name: &str) -> Result<()> {
    // `Repository::remote_rename` returns the list of non-default refspecs
    // that couldn't be auto-renamed and need manual attention — none of our
    // remotes use non-default refspecs yet, so there's nothing to surface.
    repo.remote_rename(old_name, new_name)?;
    Ok(())
}

pub fn set_remote_url(repo: &Repository, name: &str, url: &str) -> Result<()> {
    repo.remote_set_url(name, url)?;
    Ok(())
}

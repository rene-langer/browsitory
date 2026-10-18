//! Persists the undo/redo stacks to `<gitdir>/browsitory/undo.json` and keeps every object they
//! reference reachable from `refs/browsitory/undo/<worktree-key>` so `git gc` can't prune them.
// Unused until the UndoRecorder (next task) wires the store in.
#![cfg_attr(not(test), allow(dead_code))]
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use git2::{ObjectType, Oid, Repository, Signature};
use git_core::undo::{HeadState, RepoSnapshot};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub(super) const UNDO_LIMIT: usize = 50;
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoEntry {
    pub label: String,
    pub before: RepoSnapshot,
    pub after: RepoSnapshot,
}

#[derive(Debug, Default)]
pub(super) struct UndoStacks {
    pub undo: Vec<UndoEntry>,
    pub redo: Vec<UndoEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
enum StoredHead {
    Branch(String),
    Detached(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshot {
    head: StoredHead,
    refs: BTreeMap<String, String>,
    index_tree: String,
    worktree_tree: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    label: String,
    before: StoredSnapshot,
    after: StoredSnapshot,
}

#[derive(Serialize, Deserialize)]
struct StoredFile {
    version: u32,
    undo: Vec<StoredEntry>,
    redo: Vec<StoredEntry>,
}

impl From<&RepoSnapshot> for StoredSnapshot {
    fn from(snapshot: &RepoSnapshot) -> Self {
        Self {
            head: match &snapshot.head {
                HeadState::Branch(name) => StoredHead::Branch(name.clone()),
                HeadState::Detached(oid) => StoredHead::Detached(oid.clone()),
            },
            refs: snapshot.refs.clone(),
            index_tree: snapshot.index_tree.clone(),
            worktree_tree: snapshot.worktree_tree.clone(),
        }
    }
}

impl From<StoredSnapshot> for RepoSnapshot {
    fn from(stored: StoredSnapshot) -> Self {
        Self {
            head: match stored.head {
                StoredHead::Branch(name) => HeadState::Branch(name),
                StoredHead::Detached(oid) => HeadState::Detached(oid),
            },
            refs: stored.refs,
            index_tree: stored.index_tree,
            worktree_tree: stored.worktree_tree,
        }
    }
}

fn to_stored(entries: &[UndoEntry]) -> Vec<StoredEntry> {
    entries
        .iter()
        .map(|entry| StoredEntry {
            label: entry.label.clone(),
            before: (&entry.before).into(),
            after: (&entry.after).into(),
        })
        .collect()
}

fn from_stored(entries: Vec<StoredEntry>) -> Vec<UndoEntry> {
    entries
        .into_iter()
        .map(|entry| UndoEntry {
            label: entry.label,
            before: entry.before.into(),
            after: entry.after.into(),
        })
        .collect()
}

pub(super) struct UndoStore {
    file: PathBuf,
    anchor_ref: String,
    keep_prefix: String,
    mtime: Option<SystemTime>,
}

impl UndoStore {
    pub fn open(repo: &Repository) -> (Self, UndoStacks) {
        let key = worktree_key(repo);
        let mut store = Self {
            file: repo.path().join("browsitory").join("undo.json"),
            anchor_ref: format!("refs/browsitory/undo/{key}"),
            keep_prefix: format!("refs/browsitory/undo/{key}-keep/"),
            mtime: None,
        };
        let stacks = store.load();
        (store, stacks)
    }

    pub fn reload_if_changed(&mut self, stacks: &mut UndoStacks) {
        if modified(&self.file) != self.mtime {
            *stacks = self.load();
        }
    }

    pub fn save(&mut self, repo: &Repository, stacks: &UndoStacks) -> Result<(), String> {
        let file = StoredFile {
            version: FORMAT_VERSION,
            undo: to_stored(&stacks.undo),
            redo: to_stored(&stacks.redo),
        };
        let json = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
        let dir = self.file.parent().expect("undo file has a parent");
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let temp = dir.join("undo.json.tmp");
        fs::write(&temp, json).map_err(|error| error.to_string())?;
        fs::rename(&temp, &self.file).map_err(|error| error.to_string())?;
        self.mtime = modified(&self.file);
        self.write_anchor(repo, stacks)
            .map_err(|error| error.to_string())
    }

    fn load(&mut self) -> UndoStacks {
        self.mtime = modified(&self.file);
        let Ok(bytes) = fs::read(&self.file) else {
            return UndoStacks::default();
        };
        match serde_json::from_slice::<StoredFile>(&bytes) {
            Ok(file) if file.version == FORMAT_VERSION => UndoStacks {
                undo: from_stored(file.undo),
                redo: from_stored(file.redo),
            },
            Ok(file) => {
                eprintln!(
                    "browsitory: ignoring undo history with unknown version {}",
                    file.version
                );
                UndoStacks::default()
            }
            Err(error) => {
                eprintln!("browsitory: ignoring unreadable undo history: {error}");
                UndoStacks::default()
            }
        }
    }

    fn write_anchor(&self, repo: &Repository, stacks: &UndoStacks) -> Result<(), git2::Error> {
        for reference in repo.references_glob(&format!("{}*", self.keep_prefix))? {
            reference?.delete()?;
        }
        let entries: Vec<&UndoEntry> = stacks.undo.iter().chain(stacks.redo.iter()).collect();
        if entries.is_empty() {
            if let Ok(mut anchor) = repo.find_reference(&self.anchor_ref) {
                anchor.delete()?;
            }
            return Ok(());
        }

        let mut builder = repo.treebuilder(None)?;
        let mut referenced: BTreeSet<Oid> = BTreeSet::new();
        for (n, entry) in entries.iter().enumerate() {
            for (side, snapshot) in [("before", &entry.before), ("after", &entry.after)] {
                builder.insert(
                    format!("{n}-{side}-index"),
                    Oid::from_str(&snapshot.index_tree)?,
                    0o040000,
                )?;
                if let Some(tree) = &snapshot.worktree_tree {
                    builder.insert(
                        format!("{n}-{side}-worktree"),
                        Oid::from_str(tree)?,
                        0o040000,
                    )?;
                }
                if let HeadState::Detached(oid) = &snapshot.head {
                    referenced.insert(Oid::from_str(oid)?);
                }
                for oid in snapshot.refs.values() {
                    referenced.insert(Oid::from_str(oid)?);
                }
            }
        }
        let tree = repo.find_tree(builder.write()?)?;

        let mut parents = Vec::new();
        let mut keep = 0;
        for oid in referenced {
            let Ok(object) = repo.find_object(oid, None) else {
                continue;
            };
            if object.kind() == Some(ObjectType::Commit) {
                parents.push(object.peel_to_commit()?);
            } else {
                // A commit can't parent a tag object (or a tag pointing at a tree/blob); pin it
                // with its own ref instead.
                repo.reference(
                    &format!("{}{keep}", self.keep_prefix),
                    oid,
                    true,
                    "browsitory undo keep",
                )?;
                keep += 1;
            }
        }
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        let signature = Signature::now("Browsitory", "browsitory@localhost")?;
        let anchor = repo.commit(
            None,
            &signature,
            &signature,
            "browsitory undo anchor",
            &tree,
            &parent_refs,
        )?;
        repo.reference(&self.anchor_ref, anchor, true, "browsitory undo anchor")?;
        Ok(())
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    fs::metadata(file).and_then(|meta| meta.modified()).ok()
}

/// `main` for the main worktree, else the linked worktree's gitdir name (`.git/worktrees/<name>`),
/// reduced to characters that are always valid in a ref name.
fn worktree_key(repo: &Repository) -> String {
    if !repo.is_worktree() {
        return "main".to_string();
    }
    let name = repo
        .path()
        .components()
        .next_back()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| "worktree".to_string());
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::undo::{capture, SnapshotScope};
    use std::process::Command as Process;
    use tempfile::TempDir;

    fn repo_with_commit() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        (dir, repo)
    }

    fn entry(repo: &Repository, label: &str) -> UndoEntry {
        let snapshot = capture(repo, SnapshotScope::Full).unwrap();
        UndoEntry {
            label: label.into(),
            before: snapshot.clone(),
            after: snapshot,
        }
    }

    #[test]
    fn saves_and_reloads_both_stacks() {
        let (_dir, repo) = repo_with_commit();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty() && stacks.redo.is_empty());
        stacks.undo.push(entry(&repo, "Stage a.txt"));
        stacks.redo.push(entry(&repo, "Commit “x”"));
        store.save(&repo, &stacks).unwrap();
        let (_, reloaded) = UndoStore::open(&repo);
        assert_eq!(reloaded.undo, stacks.undo);
        assert_eq!(reloaded.redo, stacks.redo);
        assert!(repo.find_reference("refs/browsitory/undo/main").is_ok());
    }

    #[test]
    fn a_corrupt_file_starts_empty() {
        let (_dir, repo) = repo_with_commit();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(repo.path().join("browsitory/undo.json"), "{not json").unwrap();
        let (_, stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty());
    }

    #[test]
    fn an_unknown_version_starts_empty() {
        let (_dir, repo) = repo_with_commit();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(
            repo.path().join("browsitory/undo.json"),
            r#"{"version":99,"undo":[],"redo":[]}"#,
        )
        .unwrap();
        let (_, stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty());
    }

    #[test]
    fn empty_stacks_remove_the_anchor_refs() {
        let (_dir, repo) = repo_with_commit();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        stacks.undo.push(entry(&repo, "Stage a.txt"));
        store.save(&repo, &stacks).unwrap();
        stacks.undo.clear();
        store.save(&repo, &stacks).unwrap();
        assert!(repo.find_reference("refs/browsitory/undo/main").is_err());
    }

    #[test]
    fn reload_if_changed_picks_up_another_writer() {
        let (_dir, repo) = repo_with_commit();
        let (mut first, mut first_stacks) = UndoStore::open(&repo);
        let (mut second, mut second_stacks) = UndoStore::open(&repo);
        second_stacks.undo.push(entry(&repo, "Stage a.txt"));
        // Ensure a different mtime on coarse-grained filesystems.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        second.save(&repo, &second_stacks).unwrap();
        first.reload_if_changed(&mut first_stacks);
        assert_eq!(first_stacks.undo.len(), 1);
    }

    #[test]
    fn anchor_keeps_a_deleted_branch_commit_alive_across_gc() {
        if Process::new("git").arg("--version").output().is_err() {
            eprintln!("git CLI not found; skipping gc test");
            return;
        }
        let (dir, repo) = repo_with_commit();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = repo.signature().unwrap();
        let orphan = repo
            .commit(
                None,
                &sig,
                &sig,
                "only on feature",
                &head.tree().unwrap(),
                &[&head],
            )
            .unwrap();
        repo.reference("refs/heads/feature", orphan, true, "create")
            .unwrap();
        let tag_target = repo.find_object(orphan, None).unwrap();
        let tag = repo
            .tag("v1", &tag_target, &sig, "annotated", false)
            .unwrap();
        let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
        repo.find_reference("refs/heads/feature")
            .unwrap()
            .delete()
            .unwrap();
        repo.find_reference("refs/tags/v1")
            .unwrap()
            .delete()
            .unwrap();
        let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        stacks.undo.push(UndoEntry {
            label: "Delete branch feature".into(),
            before,
            after,
        });
        store.save(&repo, &stacks).unwrap();
        let status = Process::new("git")
            .args([
                "-c",
                "gc.reflogExpire=now",
                "-c",
                "gc.reflogExpireUnreachable=now",
            ])
            .args(["reflog", "expire", "--all", "--expire=now"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let status = Process::new("git")
            .args(["gc", "--prune=now", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let reopened = Repository::open(dir.path()).unwrap();
        assert!(
            reopened.find_commit(orphan).is_ok(),
            "deleted branch commit was pruned"
        );
        assert!(
            reopened.find_tag(tag).is_ok(),
            "deleted annotated tag object was pruned"
        );
    }
}

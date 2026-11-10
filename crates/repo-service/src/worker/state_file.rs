//! One persisted paused operation under `<gitdir>/browsitory` (#487): the file a
//! [`StateCodec`] names, written atomically ([`write_atomically`]) and read on every use. A file
//! that is unreadable, malformed, of another version, or that doesn't describe commits of this
//! repository is ignored (logged once), never fatal: a corrupt file must not wedge the
//! repository. `RebaseStore` and `SequenceStore` are this type over their codec.
use super::atomic_file::write_atomically;
use std::cell::RefCell;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use git2::Repository;

/// The format of one state file: where it lives and how its bytes map to the state. The
/// implementations delegate to `git_core`'s `*_file` modules, which `repo::branches_held` also
/// reads.
pub(super) trait StateCodec {
    type State;
    /// The word the "ignoring … state" log message names the file by.
    const WHAT: &'static str;
    /// The file in a worktree's own git folder (`git_dir`).
    fn file(git_dir: &Path) -> PathBuf;
    fn encode(state: &Self::State) -> Result<Vec<u8>, String>;
    /// The state in `bytes`, checked against `repo`; `Err` says why it can't be used.
    fn decode(bytes: &[u8], repo: &Repository) -> Result<Self::State, String>;
}

pub(super) struct StateFile<C: StateCodec> {
    file: PathBuf,
    /// The last "ignoring" message logged, so a bad file is reported once, not on every command.
    last_warning: RefCell<Option<String>>,
    codec: PhantomData<C>,
}

impl<C: StateCodec> StateFile<C> {
    /// `repo.path()` is the worktree's own git dir, so every worktree has its own file. Other
    /// worktrees load it too, to protect its branch (#291, #293).
    pub fn open(repo: &Repository) -> Self {
        Self {
            file: C::file(repo.path()),
            last_warning: RefCell::new(None),
            codec: PhantomData,
        }
    }

    /// The stored state, or `None` when there is none or the file can't be used. The codec
    /// checks it against `repo`; whether HEAD and the branch still match is the operation's job:
    /// a stale but well-formed state is still reported, so the user can see it and abort it.
    pub fn load(&self, repo: &Repository) -> Option<C::State> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.last_warning.replace(None);
                return None;
            }
            Err(error) => return self.ignore(format!("can't read it: {error}")),
        };
        match C::decode(&bytes, repo) {
            Ok(state) => {
                self.last_warning.replace(None);
                Some(state)
            }
            Err(reason) => self.ignore(reason),
        }
    }

    fn ignore(&self, reason: String) -> Option<C::State> {
        let message = format!(
            "browsitory: ignoring {} state {}: {reason}",
            C::WHAT,
            self.file.display()
        );
        if self.last_warning.borrow().as_deref() != Some(message.as_str()) {
            eprintln!("{message}");
            self.last_warning.replace(Some(message));
        }
        None
    }

    pub fn save(&self, state: &C::State) -> Result<(), String> {
        let json = C::encode(state)?;
        let dir = self.file.parent().expect("state file has a parent");
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        write_atomically(&self.file, &json).map_err(|error| error.to_string())?;
        self.last_warning.replace(None);
        Ok(())
    }

    /// Deletes the file; a missing file is fine.
    pub fn clear(&self) -> Result<(), String> {
        match fs::remove_file(&self.file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => {
                self.last_warning.replace(None);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A codec whose state is a string, stored as is; `bad…` doesn't decode.
    struct Text;

    impl StateCodec for Text {
        type State = String;
        const WHAT: &'static str = "text";
        fn file(git_dir: &Path) -> PathBuf {
            git_dir.join("browsitory").join("text.json")
        }
        fn encode(state: &String) -> Result<Vec<u8>, String> {
            if state.starts_with("unencodable") {
                Err("can't encode".into())
            } else {
                Ok(state.as_bytes().to_vec())
            }
        }
        fn decode(bytes: &[u8], _repo: &Repository) -> Result<String, String> {
            let text = String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())?;
            if text.starts_with("bad") {
                Err(format!("rejected {text}"))
            } else {
                Ok(text)
            }
        }
    }

    fn repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        (dir, repo)
    }

    fn warning(store: &StateFile<Text>) -> Option<String> {
        store.last_warning.borrow().clone()
    }

    #[test]
    fn opens_the_codecs_file_in_the_worktrees_git_dir() {
        let (_dir, repo) = repo();
        let store = StateFile::<Text>::open(&repo);
        assert_eq!(store.file, repo.path().join("browsitory/text.json"));
    }

    #[test]
    fn saves_loads_and_clears() {
        let (_dir, repo) = repo();
        let store = StateFile::<Text>::open(&repo);
        assert_eq!(store.load(&repo), None);
        store.save(&"one".to_string()).unwrap();
        assert_eq!(fs::read(&store.file).unwrap(), b"one");
        assert_eq!(store.load(&repo), Some("one".to_string()));
        store.clear().unwrap();
        assert!(!store.file.exists());
        assert_eq!(store.load(&repo), None);
        store.clear().unwrap();
    }

    #[test]
    fn an_encode_error_is_returned_and_writes_nothing() {
        let (_dir, repo) = repo();
        let store = StateFile::<Text>::open(&repo);
        assert_eq!(
            store.save(&"unencodable".to_string()),
            Err("can't encode".to_string())
        );
        assert!(!store.file.exists());
    }

    #[test]
    fn a_bad_file_is_reported_once_until_it_changes_or_goes() {
        let (_dir, repo) = repo();
        let store = StateFile::<Text>::open(&repo);
        fs::create_dir_all(store.file.parent().unwrap()).unwrap();
        fs::write(&store.file, "bad one").unwrap();
        assert_eq!(store.load(&repo), None);
        let first = warning(&store).unwrap();
        assert_eq!(
            first,
            format!(
                "browsitory: ignoring text state {}: rejected bad one",
                store.file.display()
            )
        );
        assert_eq!(store.load(&repo), None);
        assert_eq!(warning(&store), Some(first.clone()));

        fs::write(&store.file, "bad two").unwrap();
        assert_eq!(store.load(&repo), None);
        assert!(warning(&store).unwrap().ends_with("rejected bad two"));

        // A good load, a missing file, a save and a clear each forget the warning, so the same
        // bad file is reported again afterwards.
        fs::write(&store.file, "good").unwrap();
        assert_eq!(store.load(&repo), Some("good".to_string()));
        assert_eq!(warning(&store), None);

        let reset_by: [&dyn Fn(); 3] = [
            &|| {
                fs::remove_file(&store.file).unwrap();
                assert_eq!(store.load(&repo), None);
            },
            &|| store.save(&"saved".to_string()).unwrap(),
            &|| store.clear().unwrap(),
        ];
        for reset in reset_by {
            fs::write(&store.file, "bad one").unwrap();
            assert_eq!(store.load(&repo), None);
            assert_eq!(warning(&store), Some(first.clone()));
            reset();
            assert_eq!(warning(&store), None);
        }
    }

    #[test]
    fn an_unreadable_file_is_ignored_and_reported() {
        let (_dir, repo) = repo();
        let store = StateFile::<Text>::open(&repo);
        fs::create_dir_all(&store.file).unwrap();
        assert_eq!(store.load(&repo), None);
        assert!(warning(&store).unwrap().contains(": can't read it: "));
        assert!(store.save(&"x".to_string()).is_err());
        assert!(store.clear().is_err());
        // A failed save or clear keeps the warning: the file is still bad.
        assert!(warning(&store).is_some());
    }
}

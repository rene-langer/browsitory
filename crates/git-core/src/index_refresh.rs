//! Refreshing the index's stat data on a status read (#841). Stub.

use std::path::Path;

/// What identifies an index file's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fingerprint;

impl Fingerprint {
    pub(crate) fn read(_path: &Path) -> std::io::Result<Self> {
        todo!()
    }
}

/// Replaces the index at `index_path` with `refreshed` under `index.lock`.
pub(crate) fn commit(_index_path: &Path, _base: &Fingerprint, _refreshed: &[u8]) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index file's bytes: `body`, then a 20-byte trailer (`fill` repeated).
    fn index_bytes(body: &str, fill: u8) -> Vec<u8> {
        let mut bytes = body.as_bytes().to_vec();
        bytes.extend([fill; 20]);
        bytes
    }

    fn setup(content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("index");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn commit_replaces_an_unchanged_index_and_releases_the_lock() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let refreshed = index_bytes("new", 2);

        assert!(commit(&path, &base, &refreshed));

        assert_eq!(std::fs::read(&path).unwrap(), refreshed);
        assert!(!dir.path().join("index.lock").exists());
    }

    #[test]
    fn commit_leaves_an_index_that_changed_meanwhile() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let theirs = index_bytes("theirs", 3);
        std::fs::write(&path, &theirs).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert_eq!(std::fs::read(&path).unwrap(), theirs);
        assert!(!dir.path().join("index.lock").exists());
    }

    /// `index.skipHash` writes a trailer of zeros: the content tells the versions apart.
    #[test]
    fn commit_tells_skip_hash_indexes_apart_by_their_content() {
        let (_dir, path) = setup(&index_bytes("old", 0));
        let base = Fingerprint::read(&path).unwrap();
        let theirs = index_bytes("odd", 0);
        std::fs::write(&path, &theirs).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 0)));

        assert_eq!(std::fs::read(&path).unwrap(), theirs);
    }

    #[test]
    fn commit_skips_a_held_lock_and_leaves_it_in_place() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let lock = dir.path().join("index.lock");
        std::fs::write(&lock, "theirs").unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert_eq!(std::fs::read(&path).unwrap(), index_bytes("old", 1));
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "theirs");
    }

    #[test]
    fn commit_skips_a_missing_index() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert!(!path.exists());
        assert!(!dir.path().join("index.lock").exists());
    }
}

//! Atomic writes of Browsitory's state files under `<gitdir>/browsitory` (#172).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A sibling of `file` named `<name>.<pid>.<n>.tmp`: `n` comes from a process-wide counter, so
/// two workers in one process (or two threads of one) never share a temp file, and the pid
/// keeps two processes apart.
pub(super) fn temp_name(file: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.with_file_name(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Writes `bytes` to `file` atomically: a unique sibling temp file ([`temp_name`]), `sync_all`,
/// `rename`. Readers see the old content or the new one, never a torn write. The temp file is
/// removed on failure. The folder must exist.
pub(super) fn write_atomically(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = temp_name(file);
    let write = || -> std::io::Result<()> {
        let mut handle = std::fs::File::create(&temp)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
        drop(handle);
        std::fs::rename(&temp, file)
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
            .collect()
    }

    #[test]
    fn writes_the_bytes_and_leaves_no_temp_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("state.json");
        write_atomically(&file, b"one").unwrap();
        write_atomically(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn temp_names_differ_between_calls_in_one_process() {
        let file = Path::new("/x/undo.json");
        let (a, b) = (temp_name(file), temp_name(file));
        assert_ne!(a, b);
        let pid = std::process::id().to_string();
        for name in [&a, &b] {
            let name = name.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with("undo.json."), "{name}");
            assert!(name.ends_with(".tmp"), "{name}");
            assert!(name.contains(&pid), "{name}");
        }
        assert_eq!(a.parent(), file.parent());
    }

    #[test]
    fn concurrent_writers_in_one_process_never_fail() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("undo.json");
        let threads: Vec<_> = (0..8)
            .map(|thread| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for write in 0..50 {
                        let json = format!("{{\"thread\":{thread},\"write\":{write}}}");
                        write_atomically(&file, json.as_bytes()).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap();
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn a_failed_rename_removes_the_temp_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("state.json");
        std::fs::create_dir(&file).unwrap();
        std::fs::write(file.join("inside"), "x").unwrap();
        assert!(write_atomically(&file, b"one").is_err());
        assert!(temp_files(dir.path()).is_empty());
    }
}

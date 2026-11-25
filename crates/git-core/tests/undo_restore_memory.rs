//! #1089: Undo and Redo of a discard check and write a big file in pieces: neither the file on
//! disk nor the stored blob is held in memory whole. Its own test binary, so no other test's
//! memory use counts. Linux: the peak resident size (`VmHWM`) covers libgit2's allocations too.
#![cfg(target_os = "linux")]

mod common;

use std::io::Write;

use common::{commit_all, init_repo, write_file};
use git_core::undo::{capture, restore, SnapshotScope};

const BIG: usize = 64 << 20;
const MARGIN: u64 = 16 << 20;

/// A field of `/proc/self/status`, in bytes.
fn status_bytes(field: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status
        .lines()
        .find(|line| line.starts_with(field))
        .unwrap_or_else(|| panic!("{field} in /proc/self/status"));
    let kb: u64 = line
        .split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap();
    kb * 1024
}

/// The peak resident size from here on: reset to the current size where the kernel allows it
/// (`clear_refs` 5), so only what follows counts.
fn peak_base() -> u64 {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
    status_bytes("VmHWM:")
}

/// `big.bin`'s contents, written in pieces so this test never holds them either.
fn write_big(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let chunk: Vec<u8> = (0..1 << 20).map(|i| (i % 251) as u8).collect();
    for _ in 0..BIG / chunk.len() {
        file.write_all(&chunk).unwrap();
    }
}

fn hash_object(dir: &std::path::Path, path: &str) -> String {
    let output = test_support::git(dir, &["hash-object", path]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn undo_and_redo_of_a_discard_never_hold_a_big_file_whole() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "data.bin", "small\n");
    commit_all(&repo, "seed");
    write_big(&dir.path().join("data.bin"));
    let big = hash_object(dir.path(), "data.bin");

    let before = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    git_core::stage::discard_file(&repo, "data.bin").unwrap();
    let after = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data.bin")).unwrap(),
        "small\n"
    );

    let base = peak_base();
    restore(&repo, &after, &before, "undo").unwrap();
    let undo_peak = status_bytes("VmHWM:").saturating_sub(base);
    assert_eq!(hash_object(dir.path(), "data.bin"), big);
    assert!(
        undo_peak < MARGIN,
        "Undo raised the peak resident size by {undo_peak} bytes for a {BIG}-byte file"
    );

    let base = peak_base();
    restore(&repo, &before, &after, "redo").unwrap();
    let redo_peak = status_bytes("VmHWM:").saturating_sub(base);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data.bin")).unwrap(),
        "small\n"
    );
    assert!(
        redo_peak < MARGIN,
        "Redo raised the peak resident size by {redo_peak} bytes for a {BIG}-byte file"
    );
}

//! #212: marking a conflicted file resolved streams it from the opened handle into the object
//! database instead of holding the whole file in memory. Its own test binary, with a counting
//! allocator, so no other test's allocations interleave.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{commit_all, init_repo, write_file};

/// Counts the bytes Rust code holds, and the most it held since the last `reset_peak`.
struct Counting;

static HELD: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let held = HELD.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(held, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.dealloc(ptr, layout) };
        HELD.fetch_sub(layout.size(), Ordering::SeqCst);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const BIG: usize = 32 << 20;

/// A conflicted `big.bin` (binary: NUL bytes), resolved by hand to `BIG` bytes of new content.
fn conflicted_big_file() -> (tempfile::TempDir, git2::Repository, Vec<u8>) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "big.bin", "base\0");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "big.bin", "feature\0");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "big.bin", "main\0");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    let resolved: Vec<u8> = (0..BIG).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.path().join("big.bin"), &resolved).unwrap();
    (dir, repo, resolved)
}

#[test]
fn a_big_file_is_staged_without_holding_it_in_memory() {
    let (_dir, repo, resolved) = conflicted_big_file();
    let expected = git2::Oid::hash_object(git2::ObjectType::Blob, &resolved).unwrap();
    drop(resolved);

    let before = HELD.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let outcome = git_core::merge::mark_resolved(&repo, "big.bin", false).unwrap();
    let peak = PEAK.load(Ordering::SeqCst) - before;

    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    let staged = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("big.bin"), 0)
        .expect("a stage-0 entry");
    assert_eq!(staged.id, expected);
    assert!(
        peak < BIG / 8,
        "held {peak} bytes at most while staging a {BIG}-byte file"
    );
}

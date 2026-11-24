//! #1065: an undo snapshot streams each work-tree file it stores from the opened handle into the
//! object database instead of holding the whole file in memory. Its own test binary, with a
//! counting allocator, so no other test's allocations interleave.

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

#[test]
fn a_big_file_is_snapshot_without_holding_it_in_memory() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "seed.txt", "seed\n");
    commit_all(&repo, "seed");
    let contents: Vec<u8> = (0..BIG).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.path().join("big.bin"), &contents).unwrap();
    let expected = git2::Oid::hash_object(git2::ObjectType::Blob, &contents).unwrap();
    drop(contents);

    let before = HELD.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let snapshot =
        git_core::undo::capture(&repo, git_core::undo::SnapshotScope::FullUncapped).unwrap();
    let peak = PEAK.load(Ordering::SeqCst) - before;

    let tree = repo
        .find_tree(git2::Oid::from_str(snapshot.worktree_tree.as_ref().unwrap()).unwrap())
        .unwrap();
    assert_eq!(
        tree.get_path(std::path::Path::new("big.bin")).unwrap().id(),
        expected
    );
    assert!(
        peak < BIG / 8,
        "held {peak} bytes at most while snapshotting a {BIG}-byte file"
    );
}

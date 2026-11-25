//! #1158: blame reads no file over its size limit (judged from the object header) and no binary
//! file; both are refused with a typed error. `blame_file_limited` is `blame_file` with another
//! limit, so most of these need no 20 MB files.

mod common;

use common::{commit_all, init_repo, write_file};
use git_core::blame::{blame_file, blame_file_limited, BlameError, BLAME_SIZE_LIMIT};

#[test]
fn the_blame_limit_is_the_diff_limit() {
    assert_eq!(BLAME_SIZE_LIMIT, 20 * 1024 * 1024);
    assert_eq!(BLAME_SIZE_LIMIT, git_core::diff::DIFF_SIZE_LIMIT);
}

#[test]
fn a_file_over_the_limit_is_refused_with_its_size() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "big.txt", "0123456789\n");
    commit_all(&repo, "add");

    let result = blame_file_limited(&repo, "HEAD", "big.txt", 10);

    match result {
        Err(BlameError::TooLarge { size, limit }) => assert_eq!((size, limit), (11, 10)),
        other => panic!("expected TooLarge, got {other:?}"),
    }
}

#[test]
fn a_file_at_the_limit_is_blamed() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "edge.txt", "one\ntwo\n");
    commit_all(&repo, "add");

    let lines = blame_file_limited(&repo, "HEAD", "edge.txt", 8).unwrap();

    let contents: Vec<&str> = lines.iter().map(|line| line.content.as_str()).collect();
    assert_eq!(contents, ["one", "two"]);
}

#[test]
fn blame_file_refuses_a_file_over_the_blame_size_limit() {
    let (dir, repo) = init_repo();
    let line = "x".repeat(1023) + "\n";
    let text = line.repeat((BLAME_SIZE_LIMIT / 1024) as usize) + "y\n";
    write_file(dir.path(), "huge.txt", &text);
    commit_all(&repo, "add");

    match blame_file(&repo, "HEAD", "huge.txt") {
        Err(BlameError::TooLarge { size, limit }) => {
            assert_eq!((size, limit), (BLAME_SIZE_LIMIT + 2, BLAME_SIZE_LIMIT));
        }
        other => panic!(
            "expected TooLarge, got {:?}",
            other.map(|lines| lines.len())
        ),
    }
}

#[test]
fn a_binary_file_is_refused() {
    let (dir, repo) = init_repo();
    std::fs::write(
        dir.path().join("image.bin"),
        b"PNG\0\x01\x02\nmore\0bytes\n",
    )
    .unwrap();
    commit_all(&repo, "add");

    let result = blame_file(&repo, "HEAD", "image.bin");

    assert!(
        matches!(result, Err(BlameError::Binary)),
        "expected Binary, got {result:?}"
    );
}

#[test]
fn a_text_file_with_invalid_utf8_is_still_blamed() {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join("latin1.txt"), b"caf\xe9\nna\xefve\n").unwrap();
    commit_all(&repo, "add");

    let lines = blame_file(&repo, "HEAD", "latin1.txt").unwrap();

    assert_eq!(lines.len(), 2);
}

#[test]
fn the_refusals_say_why() {
    let too_large = BlameError::TooLarge {
        size: 25 * 1024 * 1024 + 300 * 1024,
        limit: BLAME_SIZE_LIMIT,
    };
    assert_eq!(
        too_large.to_string(),
        "This file is 25.3 MB, over the 20 MB blame limit, so it isn't blamed."
    );
    assert_eq!(
        BlameError::Binary.to_string(),
        "This file is binary, so it has no line-by-line blame."
    );
}

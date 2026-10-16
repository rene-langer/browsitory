//! Builds a single-hunk unified patch that applies only some of a hunk's added/removed lines.
//! Pure: no repository access, so the selection rules are unit-testable on their own.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Apply the selected lines as they appear (stage).
    Forward,
    /// Undo the selected lines (unstage, discard).
    Reverse,
}

#[derive(Debug, Clone)]
pub(crate) struct PatchLine {
    /// libgit2 origin: '+', '-', ' ', or an EOF marker ('=', '>', '<').
    pub origin: char,
    pub content: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinePatchError {
    OutOfRange,
    ContextSelected,
    Empty,
    /// The selection would put a line without a trailing newline before another line on the
    /// same side — not expressible as a valid patch.
    NotApplicable,
}

pub(crate) struct HunkRef {
    pub old_start: u32,
    pub new_start: u32,
}

/// File-header facts the patch needs, taken from the forward diff's delta.
pub(crate) struct FileRef<'a> {
    pub path: &'a str,
    /// Forward old side absent (untracked / added file).
    pub old_absent: bool,
    /// Forward new side absent (deleted file).
    pub new_absent: bool,
    /// Octal mode of the side that exists, e.g. 0o100644.
    pub mode: u32,
    /// Whether a patch that empties a side absent in the forward diff may delete the file
    /// (index operations) rather than leave it empty (workdir discard never deletes a file).
    pub allow_delete: bool,
}

pub(crate) fn build(
    file: &FileRef,
    hunk: &HunkRef,
    lines: &[PatchLine],
    selected: &[u32],
    direction: Direction,
) -> Result<Vec<u8>, LinePatchError> {
    let selected: BTreeSet<usize> = selected.iter().map(|&i| i as usize).collect();
    if selected.is_empty() {
        return Err(LinePatchError::Empty);
    }
    for &i in &selected {
        let line = lines.get(i).ok_or(LinePatchError::OutOfRange)?;
        if line.origin != '+' && line.origin != '-' {
            return Err(LinePatchError::ContextSelected);
        }
    }

    let body = match direction {
        Direction::Forward => forward_body(lines, &selected),
        Direction::Reverse => reverse_body(lines, &selected),
    };

    // A line lacking its trailing newline must end its side: nothing on the same side may
    // follow it (old side = ' ' and '-', new side = ' ' and '+'). libgit2's parser is also
    // stricter than that for the new side: after a no-EOL ' ' or '+' line *nothing* may follow,
    // while a no-EOL '-' line may only be followed by '+' lines (git's canonical order).
    for (i, (origin, content)) in body.iter().enumerate() {
        if content.ends_with(b"\n") {
            continue;
        }
        let anything_disallowed_later = body[i + 1..].iter().any(|(o, _)| match origin {
            '-' => *o != '+',
            _ => true,
        });
        if anything_disallowed_later {
            return Err(LinePatchError::NotApplicable);
        }
    }

    let old_count = body.iter().filter(|(o, _)| *o != '+').count() as u32;
    let new_count = body.iter().filter(|(o, _)| *o != '-').count() as u32;
    // The preimage side is an existing side of the forward diff, so its start is exact (and its
    // line count equals that side's count in the original hunk, keeping the unified-diff
    // convention consistent: a zero-length side's start names the line *before* the hunk).
    let old_start = match direction {
        Direction::Forward => hunk.old_start,
        Direction::Reverse => hunk.new_start,
    };
    // libgit2's `apply_hunk` ignores the preimage start and matches the preimage exactly at
    // line `new_start - 1` (0-based) with no fuzzy search, so `new_start` must be the hunk's
    // real position + 1 — even when `new_count` is 0, where plain unified diff would write the
    // position itself. With a single hunk, nothing before it shifts, so position = preimage's.
    let position = if old_count == 0 {
        old_start
    } else {
        old_start.saturating_sub(1)
    };
    let new_start = position + 1;

    // The patch's preimage is the forward old side (Forward) or the forward new side (Reverse).
    // When that side doesn't exist yet the patch creates the file.
    let (preimage_absent, postimage_absent) = match direction {
        Direction::Forward => (file.old_absent, file.new_absent),
        Direction::Reverse => (file.new_absent, file.old_absent),
    };
    // The postimage side being absent means the hunk spans the whole file; only when every
    // line is gone (and the caller allows it) does the patch delete the file. Otherwise — a
    // partial selection, or `allow_delete: false` — the file stays, possibly empty.
    let deletes_file = postimage_absent && new_count == 0 && file.allow_delete;

    let mut out = Vec::new();
    let p = file.path;
    out.extend_from_slice(format!("diff --git a/{p} b/{p}\n").as_bytes());
    if preimage_absent {
        out.extend_from_slice(format!("new file mode {:o}\n--- /dev/null\n", file.mode).as_bytes());
    } else if deletes_file {
        out.extend_from_slice(format!("deleted file mode {:o}\n--- a/{p}\n", file.mode).as_bytes());
    } else {
        out.extend_from_slice(format!("--- a/{p}\n").as_bytes());
    }
    if deletes_file {
        out.extend_from_slice(b"+++ /dev/null\n");
    } else {
        out.extend_from_slice(format!("+++ b/{p}\n").as_bytes());
    }
    out.extend_from_slice(
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n").as_bytes(),
    );
    for (origin, content) in body {
        out.push(origin as u8);
        out.extend_from_slice(content);
        if !content.ends_with(b"\n") {
            out.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
    Ok(out)
}

/// Stage: selected `+` kept, unselected `+` dropped, selected `-` kept, unselected `-` → context.
/// The forward hunk's order is already canonical, so lines keep their positions.
fn forward_body<'a>(lines: &'a [PatchLine], selected: &BTreeSet<usize>) -> Vec<(char, &'a [u8])> {
    let mut body = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let picked = selected.contains(&i);
        let emitted = match line.origin {
            ' ' => Some(' '),
            '+' => picked.then_some('+'),
            '-' => Some(if picked { '-' } else { ' ' }),
            // EOF markers are re-derived from each emitted line's own trailing newline.
            _ => None,
        };
        if let Some(origin) = emitted {
            body.push((origin, line.content.as_slice()));
        }
    }
    body
}

/// Unstage/discard: the patch's preimage is the forward *new* side. Selected forward `+` → `-`,
/// unselected forward `+` → context, selected forward `-` → `+` (restored), unselected forward
/// `-` dropped. Within each change run the restored `+` lines are emitted after the run's
/// leading `-` lines (git's canonical order) and before its first context line, so a restored
/// line lands where it was removed and a no-EOL `+` line is never followed by a `-` line.
fn reverse_body<'a>(lines: &'a [PatchLine], selected: &BTreeSet<usize>) -> Vec<(char, &'a [u8])> {
    fn flush<'a>(body: &mut Vec<(char, &'a [u8])>, restored: &mut Vec<&'a [u8]>) {
        body.extend(restored.drain(..).map(|content| ('+', content)));
    }
    let mut body = Vec::new();
    let mut restored: Vec<&[u8]> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let picked = selected.contains(&i);
        let content = line.content.as_slice();
        match line.origin {
            '-' if picked => restored.push(content),
            '-' => {}
            '+' if picked => body.push(('-', content)),
            '+' | ' ' => {
                flush(&mut body, &mut restored);
                body.push((' ', content));
            }
            // EOF markers are re-derived from each emitted line's own trailing newline.
            _ => {}
        }
    }
    flush(&mut body, &mut restored);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(origin: char, s: &str) -> PatchLine {
        PatchLine {
            origin,
            content: s.as_bytes().to_vec(),
        }
    }

    fn file() -> FileRef<'static> {
        FileRef {
            path: "f.txt",
            old_absent: false,
            new_absent: false,
            mode: 0o100644,
            allow_delete: true,
        }
    }

    fn lines() -> Vec<PatchLine> {
        vec![
            l(' ', "a\n"),
            l('-', "b\n"),
            l('+', "B\n"),
            l(' ', "c\n"),
            l('+', "d\n"),
        ]
    }

    #[test]
    fn a_pure_insertion_starts_after_the_named_line() {
        // Zero-context hunk "@@ -3,0 +4,1 @@ +d": insert after old line 3.
        let lines = vec![l('+', "d\n")];
        let h = HunkRef {
            old_start: 3,
            new_start: 4,
        };
        let patch = build(&file(), &h, &lines, &[0], Direction::Forward).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.ends_with("@@ -3,0 +4,1 @@\n+d\n"), "{text}");
    }

    #[test]
    fn a_pure_deletion_keeps_new_start_at_the_hunk_position_for_libgit2() {
        // Zero-context hunk "@@ -2,1 +1,0 @@ -b": libgit2 needs new_start - 1 == 1.
        let lines = vec![l('-', "b\n")];
        let h = HunkRef {
            old_start: 2,
            new_start: 1,
        };
        let patch = build(&file(), &h, &lines, &[0], Direction::Forward).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.ends_with("@@ -2,1 +2,0 @@\n-b\n"), "{text}");
    }

    #[test]
    fn forward_keeps_selected_adds_and_turns_unselected_removes_into_context() {
        let patch = build(
            &file(),
            &HunkRef {
                old_start: 1,
                new_start: 1,
            },
            &lines(),
            &[4],
            Direction::Forward,
        )
        .unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.ends_with("@@ -1,3 +1,4 @@\n a\n b\n c\n+d\n"),
            "{text}"
        );
    }

    #[test]
    fn reverse_turns_selected_adds_into_removes_and_drops_unselected_removes() {
        let patch = build(
            &file(),
            &HunkRef {
                old_start: 1,
                new_start: 1,
            },
            &lines(),
            &[2],
            Direction::Reverse,
        )
        .unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.ends_with("@@ -1,4 +1,3 @@\n a\n-B\n c\n d\n"),
            "{text}"
        );
    }

    #[test]
    fn missing_trailing_newline_gets_a_marker() {
        let lines = vec![l(' ', "a\n"), l('+', "b")];
        let patch = build(
            &file(),
            &HunkRef {
                old_start: 1,
                new_start: 1,
            },
            &lines,
            &[1],
            Direction::Forward,
        )
        .unwrap();
        assert!(String::from_utf8(patch)
            .unwrap()
            .ends_with("+b\n\\ No newline at end of file\n"));
    }

    #[test]
    fn new_file_header_for_an_absent_old_side() {
        let f = FileRef {
            old_absent: true,
            ..file()
        };
        let lines = vec![l('+', "x\n"), l('+', "y\n")];
        let patch = build(
            &f,
            &HunkRef {
                old_start: 0,
                new_start: 1,
            },
            &lines,
            &[1],
            Direction::Forward,
        )
        .unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.contains(
                "new file mode 100644\n--- /dev/null\n+++ b/f.txt\n@@ -0,0 +1,1 @@\n+y\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn a_no_eol_line_followed_by_a_same_side_line_is_not_applicable() {
        // old "a" (no EOL) → new "a\nb\n"; select only "+b\n".
        let lines = vec![
            l('-', "a"),
            l('<', "\n\\ No newline at end of file\n"),
            l('+', "a\n"),
            l('+', "b\n"),
        ];
        let h = HunkRef {
            old_start: 1,
            new_start: 1,
        };
        assert_eq!(
            build(&file(), &h, &lines, &[3], Direction::Forward),
            Err(LinePatchError::NotApplicable)
        );
        // Selecting all changed lines is fine: "-a" + marker is the old side's last line.
        assert!(build(&file(), &h, &lines, &[0, 2, 3], Direction::Forward).is_ok());
    }

    #[test]
    fn validation_errors() {
        let h = HunkRef {
            old_start: 1,
            new_start: 1,
        };
        assert_eq!(
            build(&file(), &h, &lines(), &[], Direction::Forward),
            Err(LinePatchError::Empty)
        );
        assert_eq!(
            build(&file(), &h, &lines(), &[0], Direction::Forward),
            Err(LinePatchError::ContextSelected)
        );
        assert_eq!(
            build(&file(), &h, &lines(), &[9], Direction::Forward),
            Err(LinePatchError::OutOfRange)
        );
    }

    #[test]
    fn reverse_emits_removes_before_restored_lines_so_a_no_eol_restore_comes_last() {
        // Forward: old "a" (no EOL) → new "a\nb\n". Undo everything.
        let lines = vec![
            l('-', "a"),
            l('<', "\n\\ No newline at end of file\n"),
            l('+', "a\n"),
            l('+', "b\n"),
        ];
        let h = HunkRef {
            old_start: 1,
            new_start: 1,
        };
        let patch = build(&file(), &h, &lines, &[0, 2, 3], Direction::Reverse).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.ends_with("@@ -1,2 +1,1 @@\n-a\n-b\n+a\n\\ No newline at end of file\n"),
            "{text}"
        );
    }

    #[test]
    fn reverse_puts_a_restored_line_before_the_runs_kept_lines() {
        // Undo only "-b" of " a -b +B  c +d": b returns just before B.
        let patch = build(
            &file(),
            &HunkRef {
                old_start: 1,
                new_start: 1,
            },
            &lines(),
            &[1],
            Direction::Reverse,
        )
        .unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.ends_with("@@ -1,4 +1,5 @@\n a\n+b\n B\n c\n d\n"),
            "{text}"
        );
    }

    #[test]
    fn a_no_eol_add_followed_by_anything_is_not_applicable() {
        let lines = vec![l('+', "x"), l('-', "y\n")];
        let h = HunkRef {
            old_start: 1,
            new_start: 1,
        };
        assert_eq!(
            build(&file(), &h, &lines, &[0, 1], Direction::Forward),
            Err(LinePatchError::NotApplicable)
        );
    }

    #[test]
    fn emptying_an_absent_side_deletes_the_file_only_when_allowed() {
        let lines = vec![l('-', "x\n"), l('-', "y\n")];
        let h = HunkRef {
            old_start: 1,
            new_start: 0,
        };
        let deleted = FileRef {
            new_absent: true,
            ..file()
        };
        let text =
            String::from_utf8(build(&deleted, &h, &lines, &[0, 1], Direction::Forward).unwrap())
                .unwrap();
        assert!(
            text.contains(
                "deleted file mode 100644\n--- a/f.txt\n+++ /dev/null\n@@ -1,2 +1,0 @@\n-x\n-y\n"
            ),
            "{text}"
        );

        let kept = FileRef {
            allow_delete: false,
            ..deleted
        };
        let text =
            String::from_utf8(build(&kept, &h, &lines, &[0, 1], Direction::Forward).unwrap())
                .unwrap();
        assert!(text.contains("--- a/f.txt\n+++ b/f.txt\n"), "{text}");

        // A partial selection never deletes.
        let partial = FileRef {
            new_absent: true,
            ..file()
        };
        let text =
            String::from_utf8(build(&partial, &h, &lines, &[0], Direction::Forward).unwrap())
                .unwrap();
        assert!(text.contains("+++ b/f.txt\n"), "{text}");
    }
}

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

    let mut body: Vec<(char, &[u8])> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let picked = selected.contains(&i);
        let emitted = match (line.origin, direction) {
            (' ', _) => Some(' '),
            ('+', Direction::Forward) => picked.then_some('+'),
            ('-', Direction::Forward) => Some(if picked { '-' } else { ' ' }),
            ('+', Direction::Reverse) => Some(if picked { '-' } else { ' ' }),
            ('-', Direction::Reverse) => picked.then_some('+'),
            // EOF markers are re-derived from each emitted line's own trailing newline.
            _ => None,
        };
        if let Some(origin) = emitted {
            body.push((origin, &line.content));
        }
    }

    // A line lacking its trailing newline must be the last line of each side it belongs to
    // (old side = ' ' and '-', new side = ' ' and '+').
    for (i, (origin, content)) in body.iter().enumerate() {
        if content.ends_with(b"\n") {
            continue;
        }
        let later_on_same_side = body[i + 1..].iter().any(|(o, _)| match origin {
            ' ' => true,
            '-' => *o != '+',
            _ => *o != '-',
        });
        if later_on_same_side {
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
    // When that side doesn't exist yet the patch creates the file. The postimage always exists:
    // a partial selection never deletes a file (selecting every line leaves it empty).
    let preimage_absent = match direction {
        Direction::Forward => file.old_absent,
        Direction::Reverse => file.new_absent,
    };

    let mut out = Vec::new();
    let p = file.path;
    out.extend_from_slice(format!("diff --git a/{p} b/{p}\n").as_bytes());
    if preimage_absent {
        out.extend_from_slice(format!("new file mode {:o}\n--- /dev/null\n", file.mode).as_bytes());
    } else {
        out.extend_from_slice(format!("--- a/{p}\n").as_bytes());
    }
    out.extend_from_slice(format!("+++ b/{p}\n").as_bytes());
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
}

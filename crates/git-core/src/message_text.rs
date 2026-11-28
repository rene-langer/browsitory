//! Commit messages as UTF-8 text (#1149).
//!
//! A commit's message is stored in the encoding its `encoding` header names (UTF-8 without
//! one): `i18n.commitEncoding=ISO-8859-1`, repositories converted from SVN or CVS, Windows tools.
//! git reads such a message for a new commit (rebase, cherry-pick, amend) by re-encoding it to
//! UTF-8 (`logmsg_reencode`), then reads any byte that still isn't UTF-8 as Latin-1
//! (`verify_utf8`), and writes the new commit in UTF-8 without the header. Browsitory does the
//! same for ISO-8859-1 and Windows-1252, which cover nearly every such repository; any other
//! encoding is read as UTF-8 with the Latin-1 fallback, as git does for an encoding iconv
//! doesn't know.
//!
//! The author and committer lines are stored in the same encoding (#1214): git re-encodes the
//! whole commit, so [`name`], [`email`] and [`author`] read them the same way.

use git2::{Commit, Signature};

/// `commit`'s message as git writes it into a commit made from it.
pub(crate) fn message(commit: &Commit<'_>) -> String {
    decode(commit.message_bytes(), encoding(commit))
}

/// `commit`'s summary (the first paragraph, as `Commit::summary` gives it), as [`message`] reads
/// the message.
pub(crate) fn summary(commit: &Commit<'_>) -> String {
    decode(commit.summary_bytes().unwrap_or_default(), encoding(commit))
}

/// The name in `signature`, `commit`'s author or committer, as git shows it.
pub(crate) fn name(commit: &Commit<'_>, signature: &Signature<'_>) -> String {
    decode(signature.name_bytes(), encoding(commit))
}

/// The email in `signature`, `commit`'s author or committer, as git shows it.
pub(crate) fn email(commit: &Commit<'_>, signature: &Signature<'_>) -> String {
    decode(signature.email_bytes(), encoding(commit))
}

/// `commit`'s author for a new commit made from it (a rebase, a pick, an amend), in UTF-8 as git
/// writes it; its time is kept.
pub(crate) fn author(commit: &Commit<'_>) -> Result<Signature<'static>, git2::Error> {
    let author = commit.author();
    let (name, email) = (name(commit, &author), email(commit, &author));
    if name.as_bytes() == author.name_bytes() && email.as_bytes() == author.email_bytes() {
        return Ok(author.to_owned());
    }
    Signature::new(&name, &email, &author.when())
}

/// `bytes` as UTF-8, any byte that isn't read as Latin-1 (git's `verify_utf8`).
pub(crate) fn text(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(chunk.invalid().iter().map(|&byte| char::from(byte)));
    }
    out
}

/// After libgit2 paused a pick of `commit`: its `MERGE_MSG` starts with the commit's message as
/// stored. git writes it re-encoded, so a commit made from it (the commit box, `git commit` in a
/// terminal) gets the text; this rewrites it so. Nothing changes for a UTF-8 message.
pub(crate) fn reencode_merge_msg(
    repo: &git2::Repository,
    commit: &Commit<'_>,
) -> std::io::Result<()> {
    let raw = commit.message_bytes();
    let decoded = message(commit);
    if decoded.as_bytes() == raw {
        return Ok(());
    }
    let path = repo.path().join("MERGE_MSG");
    let Ok(stored) = std::fs::read(&path) else {
        return Ok(());
    };
    if let Some(rest) = stored.strip_prefix(raw) {
        let mut rewritten = decoded.into_bytes();
        rewritten.extend_from_slice(rest);
        std::fs::write(&path, rewritten)?;
    }
    Ok(())
}

fn encoding(commit: &Commit<'_>) -> Option<String> {
    commit
        .message_encoding()
        .ok()
        .flatten()
        .map(str::to_ascii_lowercase)
}

fn decode(bytes: &[u8], encoding: Option<String>) -> String {
    match encoding.as_deref() {
        Some("iso-8859-1" | "iso8859-1" | "iso_8859-1" | "latin1" | "latin-1" | "l1") => {
            bytes.iter().map(|&byte| char::from(byte)).collect()
        }
        Some("windows-1252" | "cp1252") => bytes.iter().map(|&byte| windows_1252(byte)).collect(),
        _ => text(bytes),
    }
}

/// Windows-1252 differs from Latin-1 only in 0x80–0x9F. Its five unassigned bytes stay the C1
/// control characters Latin-1 reads them as.
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}',
        '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}',
        '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}',
        '\u{178}',
    ];
    match byte {
        0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

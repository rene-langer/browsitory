//! How the Windows walker treats a reparse point (#150). Pure, so it is tested on every
//! platform.

pub(crate) const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
pub(crate) const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Bit 29 of a reparse tag: the reparse point names another file (a symbolic link, a junction,
/// a WSL link, …). Opening it without `FILE_FLAG_OPEN_REPARSE_POINT` would follow it.
const NAME_SURROGATE: u32 = 0x2000_0000;

pub(crate) fn is_name_surrogate(tag: u32) -> bool {
    tag & NAME_SURROGATE != 0
}

/// What the walker does with a component it opened as itself
/// (`FILE_FLAG_OPEN_REPARSE_POINT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeafAction {
    /// No reparse point: use the handle as it is.
    Plain,
    /// A link of some kind: never followed.
    Refuse,
    /// Another reparse point (OneDrive/Cloud Files placeholder, dedup, …) that a file system
    /// filter resolves: a file is reopened without the flag (so it hydrates) and must be the
    /// same file; a folder is used as it is.
    Reopen,
}

pub(crate) fn leaf_action(attributes: u32, tag: u32) -> LeafAction {
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        LeafAction::Plain
    } else if is_name_surrogate(tag) {
        LeafAction::Refuse
    } else {
        LeafAction::Reopen
    }
}

const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
const SYMLINK_FLAG_RELATIVE: u32 = 0x1;

/// The target named by a symbolic link's or junction's `REPARSE_DATA_BUFFER` (as
/// `FSCTL_GET_REPARSE_POINT` returns it), in the form `std::fs::read_link` gives (#193): the
/// substitute name, a relative symbolic link's as it is, an absolute one's NT prefix turned into
/// a DOS path (`\??\C:\x` → `C:\x`, `\??\UNC\server\x` → `\\server\x`, anything else
/// `\??\x` → `\\?\x`). (std keeps the `\\?\` form for a path too long for the legacy APIs; this
/// doesn't.) `None` for another kind of reparse point or a malformed buffer.
pub(crate) fn link_target(buf: &[u8]) -> Option<Vec<u16>> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(buf.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(buf.get(at..at + 4)?.try_into().ok()?));
    // Tag, data length, reserved; then substitute offset/length and print offset/length (bytes,
    // relative to the path buffer); a symbolic link has a flags word before its path buffer.
    let (path_buffer, relative) = match u32_at(0)? {
        IO_REPARSE_TAG_SYMLINK => (20, u32_at(16)? & SYMLINK_FLAG_RELATIVE != 0),
        IO_REPARSE_TAG_MOUNT_POINT => (16, false),
        _ => return None,
    };
    let start = path_buffer + usize::from(u16_at(8)?);
    let length = usize::from(u16_at(10)?);
    let name: Vec<u16> = buf
        .get(start..start + length)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&unit| u16::from_le_bytes(unit))
        .collect();
    let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let Some(rest) = name
        .strip_prefix(wide(r"\??\").as_slice())
        .filter(|_| !relative)
    else {
        return Some(name);
    };
    if let Some(share) = rest.strip_prefix(wide(r"UNC\").as_slice()) {
        return Some([wide(r"\\").as_slice(), share].concat());
    }
    if matches!(rest, [_, colon, separator, ..] if *colon == u16::from(b':') && *separator == u16::from(b'\\'))
    {
        return Some(rest.to_vec());
    }
    Some([wide(r"\\?\").as_slice(), rest].concat())
}

/// The names of a file with hard links, from the `FILE_LINKS_INFORMATION` buffer
/// `NtQueryInformationFile(FileHardLinkInformation)` fills (#260): for each name, the file ID of
/// the folder it is in (`ParentFileId`, the 64-bit file reference) and the name itself. `None`
/// for a malformed buffer.
pub(crate) fn hard_links(buf: &[u8]) -> Option<Vec<(u64, Vec<u16>)>> {
    let u32_at = |at: usize| Some(u32::from_le_bytes(buf.get(at..at + 4)?.try_into().ok()?));
    let u64_at = |at: usize| Some(u64::from_le_bytes(buf.get(at..at + 8)?.try_into().ok()?));
    // `BytesNeeded`, `EntriesReturned`, then the first `FILE_LINK_ENTRY_INFORMATION` (8-byte
    // aligned): `NextEntryOffset` (0 for the last), padding, `ParentFileId`, `FileNameLength`
    // (in UTF-16 units), `FileName`.
    let count = u32_at(4)?;
    let mut links = Vec::new();
    let mut entry = 8usize;
    for _ in 0..count {
        let parent = u64_at(entry + 8)?;
        let units = usize::try_from(u32_at(entry + 16)?).ok()?;
        let start = entry + 20;
        let name = buf
            .get(start..start.checked_add(units.checked_mul(2)?)?)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&unit| u16::from_le_bytes(unit))
            .collect();
        links.push((parent, name));
        match u32_at(entry)? {
            0 => break,
            next => entry = entry.checked_add(usize::try_from(next).ok()?)?,
        }
    }
    Some(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `FILE_LINKS_INFORMATION` buffer listing `links`, each entry 8-byte aligned.
    fn links_buffer(links: &[(u64, &str)]) -> Vec<u8> {
        let mut entries = Vec::new();
        for (i, (parent, name)) in links.iter().enumerate() {
            let name: Vec<u16> = name.encode_utf16().collect();
            let size = (20 + name.len() * 2).next_multiple_of(8);
            let next = if i + 1 == links.len() { 0 } else { size };
            let mut entry = Vec::new();
            entry.extend_from_slice(&u32::try_from(next).unwrap().to_le_bytes());
            entry.extend_from_slice(&0u32.to_le_bytes());
            entry.extend_from_slice(&parent.to_le_bytes());
            entry.extend_from_slice(&u32::try_from(name.len()).unwrap().to_le_bytes());
            for unit in name {
                entry.extend_from_slice(&unit.to_le_bytes());
            }
            entry.resize(size, 0);
            entries.extend(entry);
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(&u32::try_from(8 + entries.len()).unwrap().to_le_bytes());
        buf.extend_from_slice(&u32::try_from(links.len()).unwrap().to_le_bytes());
        buf.extend(entries);
        buf
    }

    fn names(buf: &[u8]) -> Option<Vec<(u64, String)>> {
        hard_links(buf).map(|links| {
            links
                .into_iter()
                .map(|(parent, name)| (parent, String::from_utf16(&name).unwrap()))
                .collect()
        })
    }

    #[test]
    fn hard_links_are_read_with_their_folders() {
        let buf = links_buffer(&[(0x5_0000_0000_0024, "f.txt"), (7, "other name.txt")]);
        assert_eq!(
            names(&buf),
            Some(vec![
                (0x5_0000_0000_0024, "f.txt".to_string()),
                (7, "other name.txt".to_string())
            ])
        );
        assert_eq!(names(&links_buffer(&[])), Some(vec![]));
    }

    #[test]
    fn malformed_hard_link_buffers_have_no_links() {
        let buf = links_buffer(&[(1, "f.txt"), (2, "g.txt")]);
        for cut in [0, 7, 15, 27, 33, buf.len() - 3] {
            assert_eq!(names(&buf[..cut]), None, "cut at {cut}");
        }
        let mut long = buf.clone();
        long[24..28].copy_from_slice(&u32::MAX.to_le_bytes()); // a name past the end
        assert_eq!(names(&long), None);
    }

    const SYMLINK: u32 = 0xA000_000C;
    const MOUNT_POINT: u32 = 0xA000_0003;
    const LX_SYMLINK: u32 = 0xA000_001D;
    const CLOUD: u32 = 0x9000_001A;
    const DEDUP: u32 = 0x8000_0013;
    const APPEXECLINK: u32 = 0x8000_001B;

    #[test]
    fn reparse_tags_are_classified() {
        for tag in [SYMLINK, MOUNT_POINT, LX_SYMLINK] {
            assert!(is_name_surrogate(tag), "{tag:#x}");
            assert_eq!(
                leaf_action(FILE_ATTRIBUTE_REPARSE_POINT, tag),
                LeafAction::Refuse
            );
            assert_eq!(
                leaf_action(FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY, tag),
                LeafAction::Refuse
            );
        }
        for tag in [CLOUD, DEDUP, APPEXECLINK] {
            assert!(!is_name_surrogate(tag), "{tag:#x}");
            assert_eq!(
                leaf_action(FILE_ATTRIBUTE_REPARSE_POINT, tag),
                LeafAction::Reopen
            );
        }
        assert_eq!(leaf_action(0, 0), LeafAction::Plain);
        assert_eq!(leaf_action(FILE_ATTRIBUTE_DIRECTORY, 0), LeafAction::Plain);
        // A tag without the reparse attribute is stale: there is no reparse point.
        assert_eq!(leaf_action(0x20, SYMLINK), LeafAction::Plain);
    }

    /// A `REPARSE_DATA_BUFFER` as `FSCTL_GET_REPARSE_POINT` returns it: the print name first
    /// and the substitute name second, as `mklink` writes them, each NUL-terminated.
    fn reparse_buffer(tag: u32, flags: Option<u32>, substitute: &str, print: &str) -> Vec<u8> {
        let substitute: Vec<u16> = substitute.encode_utf16().collect();
        let print: Vec<u16> = print.encode_utf16().collect();
        let bytes = |v: &[u16]| u16::try_from(v.len() * 2).unwrap();
        let mut path_buffer = print.clone();
        path_buffer.push(0);
        path_buffer.extend_from_slice(&substitute);
        path_buffer.push(0);
        let header = if flags.is_some() { 12 } else { 8 };
        let mut buf = Vec::new();
        buf.extend_from_slice(&tag.to_le_bytes());
        buf.extend_from_slice(&(header + bytes(&path_buffer)).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(bytes(&print) + 2).to_le_bytes());
        buf.extend_from_slice(&bytes(&substitute).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&bytes(&print).to_le_bytes());
        if let Some(flags) = flags {
            buf.extend_from_slice(&flags.to_le_bytes());
        }
        for unit in path_buffer {
            buf.extend_from_slice(&unit.to_le_bytes());
        }
        buf
    }

    fn target(buf: &[u8]) -> Option<String> {
        link_target(buf).map(|wide| String::from_utf16(&wide).unwrap())
    }

    #[test]
    fn link_targets_are_read_as_read_link_gives_them() {
        let cases = [
            (
                SYMLINK,
                Some(1),
                r"..\other\f",
                r"..\other\f",
                r"..\other\f",
            ),
            (SYMLINK, Some(1), r"\??\x", r"\??\x", r"\??\x"),
            (
                SYMLINK,
                Some(0),
                r"\??\C:\Users\x",
                r"C:\Users\x",
                r"C:\Users\x",
            ),
            (
                MOUNT_POINT,
                None,
                r"\??\D:\target",
                r"D:\target",
                r"D:\target",
            ),
            (
                SYMLINK,
                Some(0),
                r"\??\UNC\server\share\x",
                r"\\server\share\x",
                r"\\server\share\x",
            ),
            (
                MOUNT_POINT,
                None,
                r"\??\Volume{0b1c}\x",
                "",
                r"\\?\Volume{0b1c}\x",
            ),
            (SYMLINK, Some(0), r"C:\plain", r"C:\plain", r"C:\plain"),
        ];
        for (tag, flags, substitute, print, expected) in cases {
            let buf = reparse_buffer(tag, flags, substitute, print);
            assert_eq!(target(&buf).as_deref(), Some(expected), "{substitute}");
        }
    }

    #[test]
    fn other_or_malformed_reparse_buffers_have_no_link_target() {
        assert_eq!(target(&reparse_buffer(LX_SYMLINK, None, "x", "x")), None);
        assert_eq!(target(&reparse_buffer(CLOUD, None, "x", "x")), None);
        let buf = reparse_buffer(SYMLINK, Some(0), r"\??\C:\x", r"C:\x");
        for cut in [0, 3, 12, 19, buf.len() - 3] {
            assert_eq!(target(&buf[..cut]), None, "cut at {cut}");
        }
        let mut odd = buf.clone();
        odd[8] = 0xff; // the substitute offset points past the end
        assert_eq!(target(&odd), None);
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
}

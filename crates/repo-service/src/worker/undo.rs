//! Undo stack: which worker commands are recorded, under which label, and the recorder that
//! snapshots around them. See `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`.
use git_core::reset::ResetMode;
use git_core::undo::SnapshotScope;

use super::Command;

pub(super) const MAX_LABEL_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct UndoPlan {
    pub label: String,
    pub scope: SnapshotScope,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn plan_for(command: &Command) -> Option<UndoPlan> {
    use SnapshotScope::{Full, RefsAndIndex};
    let (label, scope) = match command {
        Command::StageFile { path, .. } => (with_path("Stage ", path, ""), RefsAndIndex),
        Command::UnstageFile { path, .. } => (with_path("Unstage ", path, ""), RefsAndIndex),
        Command::StageHunk { path, .. } => (with_path("Stage hunk in ", path, ""), RefsAndIndex),
        Command::UnstageHunk { path, .. } => {
            (with_path("Unstage hunk in ", path, ""), RefsAndIndex)
        }
        Command::StageLines { path, .. } => (with_path("Stage lines in ", path, ""), RefsAndIndex),
        Command::UnstageLines { path, .. } => {
            (with_path("Unstage lines in ", path, ""), RefsAndIndex)
        }
        Command::Commit { message, .. } => (quoted("Commit ", message), RefsAndIndex),
        Command::AmendCommit { message, .. } => {
            if message.trim().is_empty() {
                ("Amend last commit".to_string(), RefsAndIndex)
            } else {
                (quoted("Amend ", message), RefsAndIndex)
            }
        }
        Command::CreateBranch { name, .. } => (fit(format!("Create branch {name}")), RefsAndIndex),
        Command::DeleteBranch { name, .. } => (fit(format!("Delete branch {name}")), RefsAndIndex),
        Command::RenameBranch {
            old_name, new_name, ..
        } => (
            fit(format!("Rename branch {old_name} to {new_name}")),
            RefsAndIndex,
        ),
        Command::CreateTag { name, .. } => (fit(format!("Create tag {name}")), RefsAndIndex),
        Command::DeleteTag { name, .. } => (fit(format!("Delete tag {name}")), RefsAndIndex),
        Command::DiscardHunk { path, .. } => (with_path("Discard hunk in ", path, ""), Full),
        Command::DiscardLines { path, .. } => (with_path("Discard lines in ", path, ""), Full),
        Command::DiscardFile { path, .. } => (with_path("Discard changes to ", path, ""), Full),
        Command::DiscardAllUnstaged { .. } => ("Discard all unstaged changes".to_string(), Full),
        Command::IgnorePath { pattern, .. } => (with_path("Ignore ", pattern, ""), Full),
        Command::SwitchBranch { name, .. } => (fit(format!("Switch to {name}")), Full),
        Command::CheckoutCommit { commit_id, .. } => {
            (format!("Check out {}", short(commit_id)), Full)
        }
        Command::ResetToCommit {
            commit_id, mode, ..
        } => {
            let mode = match mode {
                ResetMode::Soft => "soft",
                ResetMode::Mixed => "mixed",
                ResetMode::Hard => "hard",
            };
            (format!("Reset to {} ({mode})", short(commit_id)), Full)
        }
        Command::CherryPick { commit_id, .. } => {
            (format!("Cherry-pick {}", short(commit_id)), Full)
        }
        Command::RevertCommit { commit_id, .. } => (format!("Revert {}", short(commit_id)), Full),
        Command::StartMerge { branch_name, .. } => (fit(format!("Merge {branch_name}")), Full),
        Command::StartRebase { onto, .. } => (fit(format!("Rebase onto {onto}")), Full),
        Command::PullCurrentUpstream { .. } => ("Pull".to_string(), Full),
        Command::RestoreReflogEntry {
            reference, new_id, ..
        } => (
            fit(format!("Restore {reference} to {}", short(new_id))),
            Full,
        ),
        _ => return None,
    };
    Some(UndoPlan { label, scope })
}

fn short(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

fn fit(label: String) -> String {
    truncate_end(&label, MAX_LABEL_CHARS)
}

fn with_path(prefix: &str, path: &str, suffix: &str) -> String {
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + suffix.chars().count());
    format!("{prefix}{}{suffix}", ellipsize_middle(path, budget))
}

fn quoted(prefix: &str, message: &str) -> String {
    let summary = message.lines().next().unwrap_or("").trim();
    // prefix + “ + summary + ”
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + 2);
    format!("{prefix}“{}”", truncate_end(summary, budget))
}

fn truncate_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn label(command: Command) -> Option<(String, SnapshotScope)> {
        plan_for(&command).map(|plan| (plan.label, plan.scope))
    }

    #[test]
    fn labels_index_and_ref_commands() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::StageFile {
                path: "src/a.ts".into(),
                reply: tx
            }),
            Some(("Stage src/a.ts".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::Commit {
                message: "Fix parser\n\nbody".into(),
                reply: tx
            }),
            Some(("Commit “Fix parser”".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::RenameBranch {
                old_name: "a".into(),
                new_name: "b".into(),
                reply: tx
            }),
            Some(("Rename branch a to b".into(), SnapshotScope::RefsAndIndex))
        );
    }

    #[test]
    fn labels_worktree_commands_with_full_scope() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::DiscardFile {
                path: "a.txt".into(),
                reply: tx
            }),
            Some(("Discard changes to a.txt".into(), SnapshotScope::Full))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::ResetToCommit {
                commit_id: "1a2b3c4d5e6f".into(),
                mode: git_core::reset::ResetMode::Hard,
                reply: tx
            }),
            Some(("Reset to 1a2b3c4 (hard)".into(), SnapshotScope::Full))
        );
    }

    #[test]
    fn read_and_transfer_commands_are_not_recorded() {
        let (tx, _rx) = channel();
        assert_eq!(label(Command::GetStatus { reply: tx }), None);
        let (tx, _rx) = channel();
        assert_eq!(label(Command::SaveStash { reply: tx }), None);
    }

    #[test]
    fn long_paths_are_middle_ellipsized_to_80_chars() {
        let path = format!("{}/file.rs", "deep/".repeat(30));
        let (tx, _rx) = channel();
        let (text, _) = label(Command::StageFile { path, reply: tx }).unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.starts_with("Stage deep/"));
        assert!(text.ends_with("/file.rs"));
        assert!(text.contains('…'));
    }

    #[test]
    fn long_summaries_are_end_truncated() {
        let (tx, _rx) = channel();
        let (text, _) = label(Command::Commit {
            message: "x".repeat(200),
            reply: tx,
        })
        .unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.ends_with("…”"));
    }

    #[test]
    fn empty_amend_message_has_a_fallback_label() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::AmendCommit {
                message: "  ".into(),
                reply: tx
            })
            .map(|(l, _)| l),
            Some("Amend last commit".into())
        );
    }
}

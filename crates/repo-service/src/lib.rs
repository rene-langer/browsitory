//! Transport-agnostic git service layer: per-repo worker threads, credential storage, and
//! forge (GitHub/Bitbucket) pull-request API access. Shared by every `RepoClient` transport —
//! the Tauri desktop app today, a JSON-RPC sidecar for the VSCode extension later. See
//! `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`.

pub mod credentials;
pub mod pull_requests;
pub mod worker;

#[cfg(test)]
mod wire_format_tests {
    use git_core::diff::DiffLineOrigin;
    use git_core::status::StatusKind;

    /// The `kind` field of the status wire DTO is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_status_kind_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
            StatusKind::Conflicted => "Conflicted",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
            StatusKind::Conflicted,
        ] {
            assert_eq!(format!("{:?}", kind), expected_status_kind_wire_value(kind));
        }
    }

    /// A diff line's `origin` wire field is produced by `format!("{:?}", origin)`, so the
    /// `Debug` output *is* the wire format. Counterpart contract: the `DiffLineOrigin` union
    /// in `frontend/src/ipc/RepoClient.ts` (`"Add" | "Remove" | "Context"`) — these must stay
    /// in sync. Exhaustive on purpose: adding a `DiffLineOrigin` variant breaks compilation
    /// here, which is the reminder to extend the TypeScript union too.
    fn expected_diff_origin_wire_value(origin: DiffLineOrigin) -> &'static str {
        match origin {
            DiffLineOrigin::Add => "Add",
            DiffLineOrigin::Remove => "Remove",
            DiffLineOrigin::Context => "Context",
        }
    }

    #[test]
    fn diff_line_origin_wire_values_match_the_typescript_union() {
        for origin in [
            DiffLineOrigin::Add,
            DiffLineOrigin::Remove,
            DiffLineOrigin::Context,
        ] {
            assert_eq!(
                format!("{:?}", origin),
                expected_diff_origin_wire_value(origin)
            );
        }
    }
}

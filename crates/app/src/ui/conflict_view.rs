use egui::{Button, ScrollArea, Ui};

use crate::state::RepoSession;
use crate::ui::diff_view::render_lines;
use crate::worker::ResolvedWith;

/// Side-by-side ours/theirs conflict view for one selected conflicted path,
/// plus "take ours"/"take theirs" resolution buttons. Reuses `diff_view`'s
/// line-rendering (`render_lines`) rather than duplicating the diff-coloring
/// logic, since a conflict side's diff (ancestor vs. ours / ancestor vs.
/// theirs) is the exact same `FileDiff`/`DiffLine` shape as a regular file
/// diff — only where it comes from (`diff_blob_sides` vs. `diff_to_file_diff`)
/// differs.
pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Merge Conflicts");

    if session.merge_conflicts.is_empty() {
        ui.weak("No unresolved conflicts.");
        return;
    }

    // Only offer a plain "Abort Merge" here when this really is a plain
    // merge — a rebase-step conflict is shown through this same view (see
    // `rebase_progress.rs`), but is aborted via `session.abort_rebase()`
    // (its own "Abort Rebase" button), not this one; `git_core::abort_merge`
    // only knows how to unwind `MERGE_HEAD`, not an in-progress rebase.
    if !session.rebase_active && ui.button("Abort Merge").clicked() {
        session.abort_merge();
        return;
    }

    let mut clicked_path = None;
    ScrollArea::vertical()
        .id_salt("conflict_path_list")
        .max_height(140.0)
        .show(ui, |ui| {
            for path in session.merge_conflicts.clone() {
                let selected = session
                    .active_conflict
                    .as_ref()
                    .is_some_and(|c| c.path == path);
                if ui.selectable_label(selected, &path).clicked() {
                    clicked_path = Some(path);
                }
            }
        });
    if let Some(path) = clicked_path {
        session.select_conflict(path);
    }

    ui.separator();

    let Some(conflict) = &session.active_conflict else {
        ui.weak("Select a conflicted file above to review it.");
        return;
    };
    let path = conflict.path.clone();

    // Collect clicks into locals rather than calling `session.resolve_conflict`
    // (a `&mut self` method) from inside these closures — `conflict` above
    // still borrows `session.active_conflict` immutably for the diff
    // rendering below, so no mutable call on `session` can happen until
    // after that borrow's last use.
    let mut take_ours = false;
    let mut take_theirs = false;
    let mut mark_resolved = false;
    ui.horizontal(|ui| {
        if ui.add(Button::new("Take Ours")).clicked() {
            take_ours = true;
        }
        if ui.add(Button::new("Take Theirs")).clicked() {
            take_theirs = true;
        }
        // `ResolvedWith::Manual` is a no-op on the git-core side — it exists
        // for exactly this case: the user has hand-edited the file (outside
        // this app, or will via their own tools) to remove the conflict
        // markers themselves, and this button just tells the worker "stage
        // whatever's on disk now" without overwriting it with either side.
        if ui.add(Button::new("Mark Resolved")).clicked() {
            mark_resolved = true;
        }
    });
    ui.small(
        "Edit the file directly for a manual resolution, then \"Mark Resolved\" to stage it as-is \
         — \"Take Ours\"/\"Take Theirs\" instead overwrite it with a full one-sided pick.",
    );

    ui.columns(2, |columns| {
        columns[0].heading("Ours");
        ScrollArea::vertical()
            .id_salt("conflict_ours")
            .show(&mut columns[0], |ui| {
                render_lines(ui, &conflict.ours);
            });

        columns[1].heading("Theirs");
        ScrollArea::vertical()
            .id_salt("conflict_theirs")
            .show(&mut columns[1], |ui| {
                render_lines(ui, &conflict.theirs);
            });
    });

    // `conflict`'s borrow of `session.active_conflict` ends here (last use
    // above), so mutating `session` below is fine.
    if take_ours {
        session.resolve_conflict(path.clone(), ResolvedWith::Ours);
    } else if take_theirs {
        session.resolve_conflict(path.clone(), ResolvedWith::Theirs);
    } else if mark_resolved {
        session.resolve_conflict(path, ResolvedWith::Manual);
    }
}

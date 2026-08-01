use egui::{Button, Ui};
use git_core::{RebaseAction, RebaseStatus};

use crate::state::RepoSession;
use crate::ui::conflict_view;

/// Live progress view for an in-progress rebase: current step, conflict
/// resolution (reusing `conflict_view` — a rebase step's conflict is
/// surfaced through the same `merge_conflicts`/`active_conflict` state a
/// merge conflict uses, since both ultimately come from the same
/// `conflict::conflicted_paths` git-core helper), and continue/abort
/// buttons.
pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Rebase in Progress");

    let total = session.rebase_plan.as_ref().map(Vec::len).unwrap_or(0);
    ui.label(format!(
        "Step {} of {}",
        (session.rebase_cursor + 1).min(total.max(1)),
        total
    ));

    if let Some(step) = session
        .rebase_plan
        .as_ref()
        .and_then(|plan| plan.get(session.rebase_cursor))
    {
        let hash = step.commit.to_string();
        let short = &hash[..hash.len().min(7)];
        let action = match &step.action {
            RebaseAction::Pick => "pick".to_string(),
            RebaseAction::Reword(_) => "reword".to_string(),
            RebaseAction::Edit => "edit".to_string(),
            RebaseAction::Squash => "squash".to_string(),
            RebaseAction::Fixup => "fixup".to_string(),
            RebaseAction::Drop => "drop".to_string(),
        };
        ui.monospace(format!("{action} {short}"));
    }

    match &session.rebase_progress {
        Some(RebaseStatus::Conflict { .. }) => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 40),
                "Conflict — resolve every path below, then Continue.",
            );
            conflict_view::show(ui, session);
        }
        Some(RebaseStatus::PausedForEdit { .. }) => {
            ui.weak(
                "Paused for edit — amend the staged changes (Staging panel) as needed, \
                 then Continue.",
            );
        }
        _ => {}
    }

    ui.separator();
    ui.horizontal(|ui| {
        let can_continue = matches!(
            session.rebase_progress,
            Some(RebaseStatus::Conflict { .. }) | Some(RebaseStatus::PausedForEdit { .. })
        ) && session.merge_conflicts.is_empty();
        if ui
            .add_enabled(can_continue, Button::new("Continue"))
            .clicked()
        {
            session.continue_rebase();
        }
        if ui.button("Abort Rebase").clicked() {
            session.abort_rebase();
        }
    });

    if let Some(error) = &session.error {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
    }
}

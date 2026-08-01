use egui::{Button, ComboBox, ScrollArea, TextEdit, Ui};
use git_core::RebaseAction;

use crate::state::RepoSession;

type ActionFactory = (&'static str, fn() -> RebaseAction);

const ACTIONS: [ActionFactory; 6] = [
    ("pick", || RebaseAction::Pick),
    ("reword", || RebaseAction::Reword(String::new())),
    ("edit", || RebaseAction::Edit),
    ("squash", || RebaseAction::Squash),
    ("fixup", || RebaseAction::Fixup),
    ("drop", || RebaseAction::Drop),
];

fn action_label(action: &RebaseAction) -> &'static str {
    match action {
        RebaseAction::Pick => "pick",
        RebaseAction::Reword(_) => "reword",
        RebaseAction::Edit => "edit",
        RebaseAction::Squash => "squash",
        RebaseAction::Fixup => "fixup",
        RebaseAction::Drop => "drop",
    }
}

/// Ordered list of commits to be rebased, one row per commit, each with a
/// pick/reword/edit/squash/fixup/drop dropdown, plus the upstream/onto
/// pickers and a "Start Rebase" button.
pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Interactive Rebase");

    ui.horizontal(|ui| {
        ui.label("Upstream:");
        ComboBox::from_id_salt("rebase_upstream")
            .selected_text(if session.rebase_upstream.is_empty() {
                "(choose a branch)"
            } else {
                session.rebase_upstream.as_str()
            })
            .show_ui(ui, |ui| {
                let branch_names: Vec<String> =
                    session.branches.iter().map(|b| b.name.clone()).collect();
                for name in branch_names {
                    if ui
                        .selectable_label(session.rebase_upstream == name, &name)
                        .clicked()
                    {
                        session.load_rebase_plan(name);
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Onto (optional):");
        ui.add(TextEdit::singleline(&mut session.rebase_onto).desired_width(160.0));
    });

    let Some(plan) = session.rebase_plan.clone() else {
        ui.weak("Choose an upstream branch to preview commits.");
        return;
    };

    if plan.is_empty() {
        ui.weak("Nothing to rebase — already up to date with upstream.");
        return;
    }

    ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for (i, step) in plan.iter().enumerate() {
            let commit_hash = step.commit.to_string();
            let short_hash = &commit_hash[..commit_hash.len().min(7)];
            let summary = session
                .commits
                .iter()
                .find(|c| c.id == step.commit)
                .map(|c| c.summary.clone())
                .unwrap_or_default();

            ui.horizontal(|ui| {
                ui.monospace(short_hash);
                ComboBox::from_id_salt(("rebase_action", i))
                    .selected_text(action_label(&step.action))
                    .show_ui(ui, |ui| {
                        for (label, make) in ACTIONS {
                            if ui
                                .selectable_label(action_label(&step.action) == label, label)
                                .clicked()
                            {
                                session.set_rebase_action(i, make());
                            }
                        }
                    });
                ui.label(&summary);
            });

            if let RebaseAction::Reword(message) = &step.action {
                let mut edited = message.clone();
                if ui
                    .add(TextEdit::singleline(&mut edited).hint_text("new commit message"))
                    .changed()
                {
                    session.set_rebase_action(i, RebaseAction::Reword(edited));
                }
            }
        }
    });

    ui.separator();
    let can_start = !session.rebase_upstream.is_empty() && !session.rebase_active;
    if ui
        .add_enabled(can_start, Button::new("Start Rebase"))
        .clicked()
    {
        session.start_rebase();
    }

    if let Some(error) = &session.error {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
    }
}

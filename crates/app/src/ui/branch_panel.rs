use egui::{Button, Color32, ScrollArea, Ui};

use crate::state::RepoSession;
use crate::ui::transfer_progress;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Branches");

    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        let mut switch_to = None;
        let mut delete = None;
        let mut rename = None;

        for branch in &session.branches {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(branch.is_head, &branch.name)
                    .on_hover_text(if branch.is_head {
                        "Current branch".to_string()
                    } else {
                        format!("Switch to {}", branch.name)
                    })
                    .clicked()
                    && !branch.is_head
                {
                    switch_to = Some(branch.name.clone());
                }

                if let Some(upstream) = &branch.upstream {
                    ui.weak(format!("-> {upstream}"));
                }

                // The currently checked-out branch can't be deleted:
                // git2 would leave HEAD dangling (see
                // `git_core::delete_branch`'s doc comment).
                if ui
                    .add_enabled(!branch.is_head, Button::new("Delete"))
                    .clicked()
                {
                    delete = Some(branch.name.clone());
                }
                if ui.button("Rename").clicked() {
                    rename = Some(branch.name.clone());
                }
            });
        }

        if let Some(name) = switch_to {
            session.switch_branch(name);
        }
        if let Some(name) = delete {
            session.delete_branch(name);
        }
        if let Some(name) = rename {
            session.rename_target = Some(name);
        }
    });

    ui.separator();

    if let Some(old_name) = session.rename_target.clone() {
        ui.horizontal(|ui| {
            ui.label(format!("Rename '{old_name}' to:"));
            ui.text_edit_singleline(&mut session.rename_input);
            if ui.button("Confirm").clicked() {
                let new_name = session.rename_input.trim().to_string();
                if !new_name.is_empty() {
                    session.rename_branch(old_name, new_name);
                }
                session.rename_target = None;
                session.rename_input.clear();
            }
            if ui.button("Cancel").clicked() {
                session.rename_target = None;
                session.rename_input.clear();
            }
        });
        ui.separator();
    }

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut session.new_branch_name);
        if ui.button("Create Branch").clicked() {
            let name = session.new_branch_name.trim().to_string();
            if !name.is_empty() {
                session.create_branch(name, None);
                session.new_branch_name.clear();
            }
        }
    });

    ui.separator();
    show_push_controls(ui, session);

    if let Some(error) = &session.error {
        ui.colored_label(Color32::from_rgb(220, 80, 80), error);
    }
}

/// Push controls: a remote picker (a dropdown once `session.remotes` is
/// populated by Workstream D's remote-CRUD foundation, otherwise a plain
/// text field to type a remote name into), a force checkbox, and a Push
/// button that pushes the currently checked-out branch. Disabled entirely
/// while a transfer is already in flight (`session.transfer_in_progress`),
/// same as the merge/rebase buttons in `main.rs` disable on `busy`.
fn show_push_controls(ui: &mut Ui, session: &mut RepoSession) {
    let busy = session.transfer_in_progress;

    ui.horizontal(|ui| {
        ui.label("Remote:");
        if session.remotes.is_empty() {
            ui.add_enabled(!busy, egui::TextEdit::singleline(&mut session.push_remote));
        } else {
            let remote_names: Vec<String> =
                session.remotes.iter().map(|r| r.name.clone()).collect();
            if session.push_remote.is_empty() {
                session.push_remote = remote_names[0].clone();
            }
            egui::ComboBox::from_id_salt("push_remote_picker")
                .selected_text(session.push_remote.clone())
                .show_ui(ui, |ui| {
                    for name in remote_names {
                        ui.selectable_value(&mut session.push_remote, name.clone(), name);
                    }
                });
        }
        ui.add_enabled(!busy, egui::Checkbox::new(&mut session.push_force, "Force"));

        // `session.current_branch` is only populated after a manual branch
        // switch (see `Event::BranchSwitched` in `state.rs`) — it's still
        // `None` right after opening a repo, so the head branch is derived
        // from `branches`' `is_head` flag instead, which `LoadBranches`
        // (sent on every repo open) always populates.
        let current_branch = session
            .branches
            .iter()
            .find(|b| b.is_head)
            .map(|b| b.name.clone());
        let can_push = !busy && !session.push_remote.trim().is_empty() && current_branch.is_some();
        if ui.add_enabled(can_push, Button::new("Push")).clicked()
            && let Some(branch) = current_branch
        {
            session.push(session.push_remote.trim().to_string(), branch);
        }
    });

    ui.horizontal(|ui| {
        ui.label("Tag:");
        ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut session.push_tag_name),
        );
        let can_push_tag = !busy
            && !session.push_remote.trim().is_empty()
            && !session.push_tag_name.trim().is_empty();
        if ui
            .add_enabled(can_push_tag, Button::new("Push Tag"))
            .clicked()
        {
            session.push_tag(
                session.push_remote.trim().to_string(),
                session.push_tag_name.trim().to_string(),
            );
        }
    });

    transfer_progress::show(ui, session);
}

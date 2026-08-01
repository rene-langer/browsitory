use egui::{Button, Color32, Ui};

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Changes");

    let staged: Vec<String> = session
        .status
        .iter()
        .filter(|f| f.staged.is_some())
        .map(|f| f.path.clone())
        .collect();
    let unstaged: Vec<String> = session
        .status
        .iter()
        .filter(|f| f.unstaged.is_some())
        .map(|f| f.path.clone())
        .collect();

    ui.label("Staged");
    for path in &staged {
        ui.horizontal(|ui| {
            if ui.small_button("-").on_hover_text("Unstage").clicked() {
                session.unstage(path.clone());
            }
            if ui
                .selectable_label(is_selected(session, path, true), path)
                .clicked()
            {
                session.select_diff(path.clone(), true);
            }
        });
    }

    ui.separator();
    ui.label("Unstaged");
    for path in &unstaged {
        ui.horizontal(|ui| {
            if ui.small_button("+").on_hover_text("Stage").clicked() {
                session.stage(path.clone());
            }
            if ui
                .selectable_label(is_selected(session, path, false), path)
                .clicked()
            {
                session.select_diff(path.clone(), false);
            }
        });
    }

    ui.separator();
    ui.label("Commit message");
    ui.text_edit_multiline(&mut session.commit_message);
    let can_commit = !staged.is_empty() && !session.commit_message.trim().is_empty();
    if ui.add_enabled(can_commit, Button::new("Commit")).clicked() {
        session.commit();
    }

    if let Some(error) = &session.error {
        ui.colored_label(Color32::from_rgb(220, 80, 80), error);
    }
}

fn is_selected(session: &RepoSession, path: &str, staged: bool) -> bool {
    session
        .selected_diff
        .as_ref()
        .is_some_and(|d| d.path == path && d.staged == staged)
}

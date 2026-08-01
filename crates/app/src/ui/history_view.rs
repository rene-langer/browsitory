use egui::{ScrollArea, Ui};

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &RepoSession) {
    ui.heading("History");
    ScrollArea::vertical().show(ui, |ui| {
        for commit in &session.commits {
            let hash = commit.id.to_string();
            let short_hash = &hash[..hash.len().min(7)];
            ui.horizontal(|ui| {
                ui.monospace(short_hash);
                ui.label(&commit.summary);
            });
            ui.small(format!("{} <{}>", commit.author_name, commit.author_email));
            ui.separator();
        }
    });
}

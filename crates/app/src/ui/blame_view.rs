use egui::{Color32, Grid, RichText, ScrollArea, Ui};
use git_core::BlameLine;

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    let Some((path, _)) = &session.blame else {
        ui.weak("Select a file's Blame button to view attribution.");
        return;
    };
    let path = path.clone();

    let mut close = false;
    ui.horizontal(|ui| {
        ui.heading(format!("Blame: {path}"));
        if ui.small_button("Close").clicked() {
            close = true;
        }
    });
    if close {
        session.blame = None;
        return;
    }

    let Some((_, lines)) = &session.blame else {
        return;
    };
    if lines.is_empty() {
        ui.spinner();
        return;
    }

    ScrollArea::vertical().show(ui, |ui| {
        Grid::new("blame_grid")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                render_rows(ui, lines);
            });
    });
}

/// Renders one row per line, collapsing consecutive lines attributed to the
/// same commit into a single visual "run": only the first row of a run shows
/// the short hash/author/summary gutter, matching real `git blame`'s style
/// of not repeating attribution on every line of an unbroken run.
fn render_rows(ui: &mut Ui, lines: &[BlameLine]) {
    let mut i = 0;
    while i < lines.len() {
        let commit = lines[i].commit;
        render_row(ui, &lines[i], true);
        i += 1;
        while i < lines.len() && lines[i].commit == commit {
            render_row(ui, &lines[i], false);
            i += 1;
        }
    }
}

fn render_row(ui: &mut Ui, line: &BlameLine, first_of_run: bool) {
    if first_of_run {
        let hash = line.commit.to_string();
        let short_hash = &hash[..hash.len().min(7)];
        let gutter = format!("{short_hash}  {}", line.author_name);
        ui.label(
            RichText::new(gutter)
                .monospace()
                .color(Color32::from_rgb(140, 150, 190)),
        )
        .on_hover_text(format!(
            "{}\n{} <{}>",
            line.summary, line.author_name, line.author_email
        ));
    } else {
        ui.label("");
    }
    ui.monospace(line.final_lineno.to_string());
    ui.monospace(&line.content);
    ui.end_row();
}

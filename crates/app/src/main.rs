mod state;
mod theme;
mod ui;
mod worker;

use state::AppState;

struct BrowsitoryApp {
    state: AppState,
}

impl BrowsitoryApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::new();
        theme::apply(&cc.egui_ctx, state.config.preferences().theme);
        Self { state }
    }
}

impl eframe::App for BrowsitoryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Some(session) = self.state.active_session() {
            session.poll_events();
        }

        egui::Panel::left("repo_list")
            .default_size(180.0)
            .show(ui, |ui| {
                ui.heading("Repositories");
                if ui.button("Open Repository...").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.state.open_repo(path, ctx.clone());
                }
                ui.separator();

                let mut clicked = None;
                for (i, session) in self.state.sessions.iter().enumerate() {
                    let selected = self.state.active == Some(i);
                    if ui.selectable_label(selected, &session.name).clicked() {
                        clicked = Some(i);
                    }
                }
                if let Some(i) = clicked {
                    self.state.active = Some(i);
                }
            });

        egui::Panel::left("staging")
            .default_size(280.0)
            .show(ui, |ui| {
                if let Some(session) = self.state.active_session() {
                    ui::staging_panel::show(ui, session);
                }
            });

        egui::Panel::bottom("history")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                if let Some(session) = self.state.active_session() {
                    ui::history_view::show(ui, session);
                }
            });

        // Minimal merge/rebase entry point: a branch picker plus "Merge"
        // (starts a merge immediately) and "Plan Rebase" (loads a read-only
        // preview into the rebase planner below, without touching the repo
        // yet — see `Command::LoadRebasePlan`'s doc comment for why that's a
        // separate step from actually starting one).
        egui::Panel::top("merge_rebase_bar").show(ui, |ui| {
            if let Some(session) = self.state.active_session() {
                let busy = session.is_merging() || session.rebase_active;
                ui.horizontal(|ui| {
                    ui.label("Branch:");
                    egui::ComboBox::from_id_salt("merge_rebase_branch")
                        .selected_text(if session.merge_target.is_empty() {
                            "(choose a branch)"
                        } else {
                            session.merge_target.as_str()
                        })
                        .show_ui(ui, |ui| {
                            for branch in session.branches.clone() {
                                if ui
                                    .selectable_label(session.merge_target == branch, &branch)
                                    .clicked()
                                {
                                    session.merge_target = branch;
                                }
                            }
                        });
                    let has_target = !session.merge_target.is_empty();
                    if ui
                        .add_enabled(has_target && !busy, egui::Button::new("Merge"))
                        .clicked()
                    {
                        session.start_merge(session.merge_target.clone());
                    }
                    if ui
                        .add_enabled(has_target && !busy, egui::Button::new("Plan Rebase"))
                        .clicked()
                    {
                        session.load_rebase_plan(session.merge_target.clone());
                    }
                });
            }
        });

        egui::CentralPanel::default().show(ui, |ui| match self.state.active_session() {
            Some(session) => {
                if session.rebase_active {
                    ui::rebase_progress::show(ui, session);
                } else if session.rebase_plan.is_some() {
                    ui::rebase_planner::show(ui, session);
                } else if session.is_merging() {
                    ui::conflict_view::show(ui, session);
                } else {
                    ui::diff_view::show(ui, session);
                }
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.weak("Open a repository to get started.");
                });
            }
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Browsitory",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(BrowsitoryApp::new(cc)))),
    )
}

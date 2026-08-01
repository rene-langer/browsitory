mod state;
mod theme;
mod ui;
mod worker;

use state::AppState;

/// How many commits `graph_view` loads per request; generous enough to
/// cover typical repo history in one page without hand-rolling pagination
/// UI for a Phase 2 feature.
const GRAPH_PAGE_SIZE: usize = 500;

#[derive(PartialEq, Eq)]
enum HistoryMode {
    History,
    Graph,
}

struct BrowsitoryApp {
    state: AppState,
    history_mode: HistoryMode,
}

impl BrowsitoryApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::new();
        theme::apply(&cc.egui_ctx, state.config.preferences().theme);
        Self {
            state,
            history_mode: HistoryMode::History,
        }
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
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.history_mode == HistoryMode::History, "History")
                        .clicked()
                    {
                        self.history_mode = HistoryMode::History;
                    }
                    if ui
                        .selectable_label(self.history_mode == HistoryMode::Graph, "Graph")
                        .clicked()
                    {
                        self.history_mode = HistoryMode::Graph;
                        if let Some(session) = self.state.active_session()
                            && session.graph.is_none()
                        {
                            session.load_graph(GRAPH_PAGE_SIZE);
                        }
                    }
                });
                ui.separator();
                if let Some(session) = self.state.active_session() {
                    match self.history_mode {
                        HistoryMode::History => ui::history_view::show(ui, session),
                        HistoryMode::Graph => ui::graph_view::show(ui, session),
                    }
                }
            });

        egui::CentralPanel::default().show(ui, |ui| match self.state.active_session() {
            Some(session) if session.blame.is_some() => ui::blame_view::show(ui, session),
            Some(session) => ui::diff_view::show(ui, session),
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

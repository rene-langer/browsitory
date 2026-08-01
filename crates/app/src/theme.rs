use egui::Context;

pub fn apply(ctx: &Context, theme: config::Theme) {
    match theme {
        config::Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
        config::Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        // System: leave egui's own default, which already tracks the OS
        // theme where the windowing backend reports one.
        config::Theme::System => {}
    }
}

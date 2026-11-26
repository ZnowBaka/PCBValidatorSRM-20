use std::path::Path;

use eframe::egui::{self, Color32};

use crate::app::PcbApp;

pub(super) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    egui::Panel::top("top_bar").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let button_width = 120.0;
            let spacing = ui.spacing().item_spacing.x;
            let height = 28.0;

            let response = ui.add_sized(
                [ui.available_width() - button_width - spacing, height],
                egui::TextEdit::singleline(&mut app.path_text)
                    .hint_text("Gerber file path + Enter (or drop a file on the window)…"),
            );
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                let path = app.path_text.trim().trim_matches('"').to_owned();
                if !path.is_empty() {
                    app.load_path(Path::new(&path));
                }
            }

            let (fill, label) = if app.expert_mode {
                (Color32::from_rgb(237, 28, 36), "Expert mode: ON")
            } else {
                (Color32::from_rgb(140, 40, 40), "Expert mode")
            };
            let button =
                egui::Button::new(egui::RichText::new(label).strong().color(Color32::WHITE))
                    .fill(fill);
            if ui.add_sized([button_width, height], button).clicked() {
                app.expert_mode = !app.expert_mode;
            }
        });
        ui.add_space(4.0);
    });
}

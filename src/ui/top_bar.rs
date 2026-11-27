use eframe::egui::{self, Color32};

use crate::app::PcbApp;

pub(super) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    egui::Panel::top("top_bar").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let button_width = 120.0;
            let height = 28.0;

            let (fill, label) = if app.expert_mode {
                (Color32::from_rgb(237, 28, 36), "Expert mode: ON")
            } else {
                (Color32::from_rgb(140, 40, 40), "Expert mode")
            };
            let browse_file_button = 
            egui::Button::new(egui::RichText::new("Browse files")
                    .strong()
                    .color(Color32::WHITE))
                        .fill(fill);

            let expert_button =
                egui::Button::new(egui::RichText::new(label)
                .strong()
                .color(Color32::WHITE))
                    .fill(fill);

            if ui
                .add_sized([button_width, height], browse_file_button)
                .clicked()
            {
                app.open_file_browser();
            }
            
            ui.add_space(ui.available_width() - button_width);

            if ui
                .add_sized([button_width, height], expert_button)
                .clicked()
            {
                app.expert_mode = !app.expert_mode;
            }
        });
        ui.add_space(4.0);
    });
}

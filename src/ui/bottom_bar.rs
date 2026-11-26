use eframe::egui::{self, Align, Layout};

use crate::app::{PcbApp, ViewMode};

pub(super) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    egui::Panel::bottom("bottom_bar").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.view_mode, ViewMode::Gerber, "Gerber\nView");
            ui.selectable_value(&mut app.view_mode, ViewMode::Toolpath, "Tool path\nview");
            if ui
                .checkbox(&mut app.transform.mirroring.x, "Mirror X")
                .on_hover_text("Flip the layer, e.g. for bottom copper")
                .changed()
            {
                app.fit_requested = true;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let button = egui::Button::new(egui::RichText::new("Generate").size(28.0).strong());
                if ui.add_sized([200.0, 52.0], button).clicked() {
                    app.generate();
                }
            });
        });
        ui.add_space(4.0);
    });
}

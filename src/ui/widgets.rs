use eframe::egui::{self, Stroke};

pub(super) fn boxed(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .stroke(Stroke::new(1.5, ui.visuals().text_color()))
        .inner_margin(8.0)
}

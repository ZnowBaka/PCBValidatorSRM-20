mod bottom_bar;
mod canvas;
mod side_panel;
mod top_bar;
mod widgets;

use eframe::egui;

use crate::app::PcbApp;

pub(crate) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    top_bar::show(app, root);
    bottom_bar::show(app, root);
    side_panel::show(app, root);
    canvas::show(app, root);
}

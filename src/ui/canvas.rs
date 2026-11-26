use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};
use gerber_viewer::{BoundingBox, GerberRenderer};

use crate::app::{PcbApp, ViewMode};
use crate::board::layer::unit_label;

pub(super) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    egui::CentralPanel::default().show(root, |ui| {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;

        let fit_key = response.hovered() && ui.input(|input| input.key_pressed(egui::Key::F));
        if app.fit_requested || response.double_clicked() || fit_key {
            let mut total: Option<BoundingBox> = None;
            for layer in app.layers.iter().filter(|layer| layer.visible) {
                if let Some(bounds) = layer.layer.try_bounding_box() {
                    let bounds = bounds.apply_transform(&app.transform);
                    match &mut total {
                        Some(total) => total.expand(&bounds),
                        None => total = Some(bounds),
                    }
                }
            }
            if let Some(bounds) = total {
                app.view.fit_view(rect, &bounds, 1.0);
            }
            app.fit_requested = false;
        }

        app.ui_state.update(ui, &rect, &response, &mut app.view);

        let origin = app.ui_state.origin_screen_pos;
        let axis = Stroke::new(1.0, Color32::from_gray(90));
        painter.line_segment(
            [
                Pos2::new(rect.left(), origin.y),
                Pos2::new(rect.right(), origin.y),
            ],
            axis,
        );
        painter.line_segment(
            [
                Pos2::new(origin.x, rect.top()),
                Pos2::new(origin.x, rect.bottom()),
            ],
            axis,
        );

        let unit = app
            .layers
            .iter()
            .find(|layer| layer.visible)
            .map(|layer| unit_label(layer.unit))
            .unwrap_or("mm");

        if app.layers.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop a Gerber file or .zip here\nor type its path above and press Enter",
                FontId::proportional(16.0),
                ui.visuals().weak_text_color(),
            );
        } else {
            for layer in app.layers.iter().filter(|layer| layer.visible) {
                let color = match app.view_mode {
                    ViewMode::Gerber => layer.color,
                    ViewMode::Toolpath => Color32::from_gray(70),
                };
                GerberRenderer::new(&app.render_config, app.view, &app.transform, &layer.layer)
                    .paint_layer(&painter, color);
            }
            if app.view_mode == ViewMode::Toolpath {
                painter.text(
                    rect.center_top() + Vec2::new(0.0, 12.0),
                    Align2::CENTER_TOP,
                    "Toolpath generation not implemented yet",
                    FontId::proportional(14.0),
                    ui.visuals().weak_text_color(),
                );
            }
        }

        if response.hovered() {
            if let Some(position) = app.ui_state.cursor_gerber_coords {
                painter.text(
                    rect.left_bottom() + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    format!("x {:9.3} {unit}   y {:9.3} {unit}", position.x, position.y),
                    FontId::monospace(12.0),
                    ui.visuals().text_color(),
                );
            }
        }
    });
}

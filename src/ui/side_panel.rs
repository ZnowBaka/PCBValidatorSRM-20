use eframe::egui;

use crate::app::PcbApp;
use crate::tools::bit::Bit;

use super::widgets::boxed;

pub(super) fn show(app: &mut PcbApp, root: &mut egui::Ui) {
    egui::Panel::right("right_panel")
        .resizable(false)
        .show(root, |ui| {
            ui.set_width(230.0);
            boxed(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong("Your bits list");
                ui.horizontal(|ui| {
                    let selected_name = app.bits[app.selected_bit].name.clone();
                    egui::ComboBox::from_id_salt("bit_select")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for (index, bit) in app.bits.iter().enumerate() {
                                ui.selectable_value(&mut app.selected_bit, index, &bit.name);
                            }
                        });
                    if ui.button("+").on_hover_text("Add bit").clicked() {
                        app.bits.push(Bit::new("New bit", 0.5));
                        app.selected_bit = app.bits.len() - 1;
                    }
                    if ui.button("−").on_hover_text("Remove bit").clicked() && app.bits.len() > 1
                    {
                        app.bits.remove(app.selected_bit);
                        app.selected_bit = app.selected_bit.min(app.bits.len() - 1);
                    }
                });
            });

            ui.add_space(6.0);

            let expert = app.expert_mode;
            let bit = &mut app.bits[app.selected_bit];
            boxed(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("bit_settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut bit.name);
                        ui.end_row();

                        ui.label("Diameter");
                        ui.add(
                            egui::DragValue::new(&mut bit.diameter_mm)
                                .speed(0.01)
                                .suffix(" mm"),
                        );
                        ui.end_row();

                        ui.label("Feedrate");
                        ui.add(
                            egui::DragValue::new(&mut bit.feedrate_mm_s)
                                .speed(0.1)
                                .suffix(" mm/s"),
                        );
                        ui.end_row();

                        ui.label("Depth/pass");
                        ui.add(
                            egui::DragValue::new(&mut bit.depth_per_pass_mm)
                                .speed(0.005)
                                .suffix(" mm"),
                        );
                        ui.end_row();

                        if expert {
                            ui.label("Plunge");
                            ui.add(
                                egui::DragValue::new(&mut bit.plunge_rate_mm_s)
                                    .speed(0.1)
                                    .suffix(" mm/s"),
                            );
                            ui.end_row();

                            ui.label("Spindle");
                            ui.add(
                                egui::DragValue::new(&mut bit.spindle_rpm)
                                    .speed(100)
                                    .suffix(" rpm"),
                            );
                            ui.end_row();

                            ui.label("Stepover");
                            ui.add(
                                egui::DragValue::new(&mut bit.stepover_pct)
                                    .speed(0.5)
                                    .suffix(" %"),
                            );
                            ui.end_row();
                        }
                    });
            });

            ui.add_space(6.0);

            boxed(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height() - 8.0);
                ui.strong("Layers");
                egui::ScrollArea::vertical()
                    .id_salt("layer_list")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for layer in &mut app.layers {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut layer.visible, "");
                                ui.color_edit_button_srgba(&mut layer.color);
                                ui.label(&layer.name);
                            });
                        }
                    });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &app.log {
                            ui.label(line);
                        }
                    });
            });
        });
}

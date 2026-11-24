use eframe::egui::{self, Align, Color32, Layout, Pos2, Sense, Stroke, Vec2};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([720.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PCB CAM",
        options,
        Box::new(|_cc| Ok(Box::new(PcbApp::default()))),
    )
}

// ---------------------------------------------------------------- data ----

#[derive(Clone)]
struct Bit {
    name: String,
    diameter_mm: f32,
    feedrate_mm_s: f32,
    depth_per_pass_mm: f32,
    // expert-only settings
    plunge_rate_mm_s: f32,
    spindle_rpm: u32,
    stepover_pct: f32,
}

impl Bit {
    fn new(name: &str, diameter_mm: f32) -> Self {
        Self {
            name: name.to_owned(),
            diameter_mm,
            feedrate_mm_s: 5.0,
            depth_per_pass_mm: 0.05,
            plunge_rate_mm_s: 1.0,
            spindle_rpm: 12_000,
            stepover_pct: 40.0,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum ViewMode {
    Gerber,
    Toolpath,
}

struct Viewport2D {
    offset: Vec2, // screen-space pan
    zoom: f32,    // pixels per mm
}

struct PcbApp {
    path_text: String,
    expert_mode: bool,
    bits: Vec<Bit>,
    selected_bit: usize,
    view_mode: ViewMode,
    viewport: Viewport2D,
    log: Vec<String>,
}

impl Default for PcbApp {
    fn default() -> Self {
        Self {
            path_text: String::new(),
            expert_mode: false,
            bits: vec![Bit::new("V-bit 30°", 0.1), Bit::new("End mill 0.8", 0.8)],
            selected_bit: 0,
            view_mode: ViewMode::Gerber,
            viewport: Viewport2D { offset: Vec2::ZERO, zoom: 4.0 },
            log: vec!["Ready.".into()],
        }
    }
}

// ----------------------------------------------------------------- app ----

impl eframe::App for PcbApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Order matters: outermost panels first, CentralPanel last.
        self.top_bar(ui);
        self.bottom_bar(ui);
        self.right_panel(ui);
        self.canvas(ui);
    }
}

fn boxed(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .stroke(Stroke::new(1.5, ui.visuals().text_color()))
        .inner_margin(8.0)
}

impl PcbApp {
    /// Top: wide text field + red "Expert mode" button
    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let button_w = 120.0;
                let spacing = ui.spacing().item_spacing.x;
                let h = 28.0;

                ui.add_sized(
                    [ui.available_width() - button_w - spacing, h],
                    egui::TextEdit::singleline(&mut self.path_text)
                        .hint_text("Gerber file / project path…"),
                );

                let (fill, label) = if self.expert_mode {
                    (Color32::from_rgb(237, 28, 36), "Expert mode: ON")
                } else {
                    (Color32::from_rgb(140, 40, 40), "Expert mode")
                };
                let btn = egui::Button::new(egui::RichText::new(label).strong().color(Color32::WHITE))
                    .fill(fill);
                if ui.add_sized([button_w, h], btn).clicked() {
                    self.expert_mode = !self.expert_mode;
                }
            });
            ui.add_space(4.0);
        });
    }

    /// Bottom: view toggles (left) + Generate (right)
    fn bottom_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("bottom_bar").show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view_mode, ViewMode::Gerber, "Gerber\nView");
                ui.selectable_value(&mut self.view_mode, ViewMode::Toolpath, "Tool path\nview");

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let generate_btn = egui::Button::new(egui::RichText::new("Generate").size(28.0).strong());
                    if ui.add_sized([200.0, 52.0], generate_btn).clicked() {
                        self.generate();
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    /// Right: bits list, selected-bit settings, spare/log box
    fn right_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("right_panel")
            .resizable(false)
            .show(root, |ui| {
                ui.set_width(230.0);
                // --- "Your bits list" header box
                boxed(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.strong("Your bits list");
                    ui.horizontal(|ui| {
                        let selected_name = self.bits[self.selected_bit].name.clone();
                        egui::ComboBox::from_id_salt("bit_select")
                            .selected_text(selected_name)
                            .show_ui(ui, |ui| {
                                for (i, b) in self.bits.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_bit, i, &b.name);
                                }
                            });
                        if ui.button("+").on_hover_text("Add bit").clicked() {
                            self.bits.push(Bit::new("New bit", 0.5));
                            self.selected_bit = self.bits.len() - 1;
                        }
                        if ui.button("−").on_hover_text("Remove bit").clicked() && self.bits.len() > 1 {
                            self.bits.remove(self.selected_bit);
                            self.selected_bit = self.selected_bit.min(self.bits.len() - 1);
                        }
                    });
                });

                ui.add_space(6.0);

                // --- tool settings box
                let expert = self.expert_mode;
                let bit = &mut self.bits[self.selected_bit];
                boxed(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("bit_settings").num_columns(2).show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut bit.name);
                        ui.end_row();

                        ui.label("Diameter");
                        ui.add(egui::DragValue::new(&mut bit.diameter_mm).speed(0.01).suffix(" mm"));
                        ui.end_row();

                        ui.label("Feedrate");
                        ui.add(egui::DragValue::new(&mut bit.feedrate_mm_s).speed(0.1).suffix(" mm/s"));
                        ui.end_row();

                        ui.label("Depth/pass");
                        ui.add(egui::DragValue::new(&mut bit.depth_per_pass_mm).speed(0.005).suffix(" mm"));
                        ui.end_row();

                        if expert {
                            ui.label("Plunge");
                            ui.add(egui::DragValue::new(&mut bit.plunge_rate_mm_s).speed(0.1).suffix(" mm/s"));
                            ui.end_row();

                            ui.label("Spindle");
                            ui.add(egui::DragValue::new(&mut bit.spindle_rpm).speed(100).suffix(" rpm"));
                            ui.end_row();

                            ui.label("Stepover");
                            ui.add(egui::DragValue::new(&mut bit.stepover_pct).speed(0.5).suffix(" %"));
                            ui.end_row();
                        }
                    });
                });

                ui.add_space(6.0);

                // --- spare box (currently: log output)
                boxed(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(ui.available_height() - 8.0);
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for line in &self.log {
                            ui.label(line);
                        }
                    });
                });
            });
    }

    /// Centre: pannable / zoomable canvas
    fn canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;

            // pan
            if response.dragged() {
                self.viewport.offset += response.drag_delta();
            }
            // zoom (scroll), keeping the point under the cursor fixed
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let factor = (scroll * 0.002).exp();
                    let old = self.viewport.zoom;
                    let new = (old * factor).clamp(0.5, 200.0);
                    if let Some(p) = response.hover_pos() {
                        let c = rect.center() + self.viewport.offset;
                        let world = (p - c) / old;
                        self.viewport.offset = (p - world * new) - rect.center();
                    }
                    self.viewport.zoom = new;
                }
            }

            let vp = &self.viewport;
            let to_screen = |p: Pos2| -> Pos2 { rect.center() + vp.offset + p.to_vec2() * vp.zoom };

            // TODO: replace with real parsed Gerber / generated toolpath data
            let (board, traces) = demo_geometry();

            match self.view_mode {
                ViewMode::Gerber => {
                    let pts: Vec<Pos2> = board.iter().map(|&p| to_screen(p)).collect();
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, Color32::GRAY)));
                    for t in &traces {
                        let pts: Vec<Pos2> = t.iter().map(|&p| to_screen(p)).collect();
                        painter.add(egui::Shape::line(
                            pts,
                            Stroke::new(0.3 * vp.zoom, Color32::from_rgb(200, 150, 50)),
                        ));
                    }
                }
                ViewMode::Toolpath => {
                    let w = self.bits[self.selected_bit].diameter_mm * vp.zoom;
                    for t in &traces {
                        let pts: Vec<Pos2> = t.iter().map(|&p| to_screen(p)).collect();
                        painter.add(egui::Shape::line(
                            pts,
                            Stroke::new(w.max(1.0), Color32::from_rgb(60, 140, 255)),
                        ));
                    }
                }
            }
        });
    }

    fn generate(&mut self) {
        let bit = &self.bits[self.selected_bit];
        self.log.push(format!(
            "Generate: bit '{}', {:.2} mm/s, {:.3} mm/pass",
            bit.name, bit.feedrate_mm_s, bit.depth_per_pass_mm
        ));
        // TODO: run toolpath generation (ideally on a background thread)
        self.view_mode = ViewMode::Toolpath;
    }
}

/// Placeholder shapes resembling the sketch (units: mm, origin centred).
fn demo_geometry() -> (Vec<Pos2>, Vec<Vec<Pos2>>) {
    let board = vec![
        Pos2::new(-10.0, -12.0),
        Pos2::new(20.0, -2.0),
        Pos2::new(13.0, 18.0),
        Pos2::new(-17.0, 8.0),
    ];
    let traces = vec![
        vec![Pos2::new(-4.0, -9.0), Pos2::new(-9.0, -2.0), Pos2::new(2.0, 1.0), Pos2::new(0.0, 9.0)],
        vec![Pos2::new(0.0, 9.0), Pos2::new(9.0, 0.0)],
        vec![Pos2::new(-12.0, 7.0), Pos2::new(0.0, 9.0)],
    ];
    (board, traces)
}

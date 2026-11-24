mod gerber;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::Path;

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
    layer: Option<gerber::GerberLayer>,
    fit_requested: bool,
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
            log: vec!["Ready. Drop a Gerber file on the window.".into()],
            layer: None,
            fit_requested: false,
        }
    }
}

// ----------------------------------------------------------------- app ----

impl eframe::App for PcbApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui);

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

                let resp = ui.add_sized(
                    [ui.available_width() - button_w - spacing, h],
                    egui::TextEdit::singleline(&mut self.path_text)
                        .hint_text("Gerber file path + Enter (or drop a file on the window)…"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let path = self.path_text.trim().trim_matches('"').to_owned();
                    if !path.is_empty() {
                        self.load_file(Path::new(&path));
                    }
                }

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

    /// Centre: pannable / zoomable canvas showing the loaded Gerber layer
    fn canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;

            // fit to view: after loading, on double-click, or with F
            let fit_key = response.hovered() && ui.input(|i| i.key_pressed(egui::Key::F));
            if self.fit_requested || response.double_clicked() || fit_key {
                self.fit_view(rect);
                self.fit_requested = false;
            }

            // pan
            if response.dragged() {
                self.viewport.offset += response.drag_delta();
            }
            // zoom (scroll wheel / pinch), keeping the point under the cursor fixed
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                let pinch = ui.input(|i| i.zoom_delta());
                let factor = (scroll * 0.002).exp() * pinch;
                if factor != 1.0 {
                    let old = self.viewport.zoom;
                    let new = (old * factor).clamp(0.01, 5000.0);
                    if let Some(p) = response.hover_pos() {
                        let c = rect.center() + self.viewport.offset;
                        let world = (p - c) / old;
                        self.viewport.offset = (p - world * new) - rect.center();
                    }
                    self.viewport.zoom = new;
                }
            }

            let zoom = self.viewport.zoom;
            let c = rect.center() + self.viewport.offset; // screen position of world origin

            // origin axes
            let axis = Stroke::new(1.0, Color32::from_gray(90));
            painter.line_segment([Pos2::new(rect.left(), c.y), Pos2::new(rect.right(), c.y)], axis);
            painter.line_segment([Pos2::new(c.x, rect.top()), Pos2::new(c.x, rect.bottom())], axis);

            match &self.layer {
                Some(layer) => {
                    let bg = ui.visuals().panel_fill;
                    let fg = match self.view_mode {
                        ViewMode::Gerber => Color32::from_rgb(200, 150, 50),
                        ViewMode::Toolpath => Color32::from_gray(70),
                    };
                    // World mm (Y up) -> screen px (Y down). Clear-polarity vertices use the
                    // background colour.
                    let mut mesh = egui::Mesh::default();
                    mesh.vertices.reserve(layer.verts.len());
                    for v in &layer.verts {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: Pos2::new(c.x + v.pos[0] * zoom, c.y - v.pos[1] * zoom),
                            uv: egui::epaint::WHITE_UV,
                            color: if v.clear { bg } else { fg },
                        });
                    }
                    mesh.indices.extend_from_slice(&layer.indices);
                    painter.add(egui::Shape::mesh(mesh));

                    if self.view_mode == ViewMode::Toolpath {
                        painter.text(
                            rect.center_top() + Vec2::new(0.0, 12.0),
                            Align2::CENTER_TOP,
                            "Toolpath generation not implemented yet",
                            FontId::proportional(14.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                None => {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Drop a Gerber file here\nor type its path above and press Enter",
                        FontId::proportional(16.0),
                        ui.visuals().weak_text_color(),
                    );
                }
            }

            // cursor position in mm
            if let Some(p) = response.hover_pos() {
                let (wx, wy) = ((p.x - c.x) / zoom, -(p.y - c.y) / zoom);
                painter.text(
                    rect.left_bottom() + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    format!("x {wx:8.3} mm   y {wy:8.3} mm   zoom {zoom:.1} px/mm"),
                    FontId::monospace(12.0),
                    ui.visuals().text_color(),
                );
            }
        });
    }

    fn fit_view(&mut self, rect: Rect) {
        let Some((min, max)) = self.layer.as_ref().and_then(|l| l.bounds) else {
            return;
        };
        let w = (max[0] - min[0]).max(1e-3);
        let h = (max[1] - min[1]).max(1e-3);
        let zoom = (rect.width() / w).min(rect.height() / h) * 0.9;
        let (cx, cy) = ((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0);
        self.viewport.zoom = zoom;
        self.viewport.offset = Vec2::new(-cx * zoom, cy * zoom);
    }

fn handle_dropped_files(&mut self, ui: &egui::Ui) {
    let dropped = ui.input(|i| i.raw.dropped_files.clone());
    for f in dropped {
        let path = f.path();
        if path.is_file() {
            self.path_text = path.display().to_string();
            self.load_file(path);
        } else {
            match f.bytes() {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.load_gerber_text("dropped file", &text);
                }
                Err(e) => self.log.push(format!("Could not read dropped file: {e}")),
            }
        }
    }
}

    fn load_file(&mut self, path: &Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                self.load_gerber_text(&name, &String::from_utf8_lossy(&bytes));
            }
            Err(e) => self.log.push(format!("Cannot read {}: {e}", path.display())),
        }
    }

    fn load_gerber_text(&mut self, name: &str, text: &str) {
        let layer = gerber::parse(text);
        match layer.bounds {
            Some((min, max)) => self.log.push(format!(
                "{name}: {:.2} x {:.2} mm, {} draws, {} flashes, {} regions",
                max[0] - min[0],
                max[1] - min[1],
                layer.n_draws,
                layer.n_flashes,
                layer.n_regions
            )),
            None => self.log.push(format!("{name}: no geometry found (is this a Gerber file?)")),
        }
        for w in layer.warnings.iter().take(5) {
            self.log.push(format!("warning: {w}"));
        }
        self.layer = Some(layer);
        self.fit_requested = true;
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

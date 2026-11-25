use std::io::{BufReader, Cursor};
use std::path::Path;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Sense, Stroke, Vec2};
use gerber_viewer::gerber_types::{Command, ExtendedCode, Unit};
use gerber_viewer::{
    BoundingBox, GerberLayer, GerberRenderer, GerberTransform, RenderConfiguration, UiState,
    ViewState,
};

mod zip_reader;
use crate::zip_reader::read_zip;

/// Substrings used to pick Gerber files out of a zip (matched against the entry name).
const ZIP_PATTERNS: &[&str] = &[
    "copper_top", "profile", "soldermask",
];

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

struct LoadedLayer {
    name: String,
    layer: GerberLayer,
    /// Coordinates in the layer are in this unit (gerber_viewer does not convert).
    unit: Unit,
    visible: bool,
    color: Color32,
}

/// Guess a colour and default visibility from the file name (KiCad / Protel naming).
fn style_for(name: &str) -> (Color32, bool) {
    let n = name.to_ascii_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
    if has(&["edge_cuts", ".gm1", ".gko", ".gml", "outline", "profile"]) {
        (Color32::from_rgb(230, 220, 90), true) // board outline
    } else if has(&["silk", "mask", "paste", ".gto", ".gbo", ".gts", ".gbs"]) {
        (Color32::from_gray(150), false) // not needed for milling
    } else if has(&["b_cu", ".gbl", "bottom"]) {
        (Color32::from_rgb(80, 140, 230), false) // bottom copper, hidden by default
    } else {
        (Color32::from_rgb(200, 150, 50), true) // top copper / unknown
    }
}

/// Parse one Gerber file into a layer. Err(reason) if it has no drawable geometry.
fn parse_layer(name: &str, bytes: Vec<u8>) -> Result<LoadedLayer, String> {
    let mut parse_err = None;
    let doc = match gerber_viewer::gerber_parser::parse(BufReader::new(Cursor::new(bytes))) {
        Ok(doc) => doc,
        Err((doc, err)) => {
            // a partial document is still returned, so use whatever parsed
            parse_err = Some(format!("{err:?}"));
            doc
        }
    };
    let commands = doc.into_commands();

    // gerber_viewer draws in file units, so remember which unit that is
    let unit = commands
        .iter()
        .find_map(|c| match c {
            Command::ExtendedCode(ExtendedCode::Unit(u)) => Some(*u),
            _ => None,
        })
        .unwrap_or(Unit::Millimeters);

    let layer = GerberLayer::new(commands);
    if layer.try_bounding_box().is_none() {
        return Err(parse_err.unwrap_or_else(|| "no geometry".into()));
    }
    let (color, visible) = style_for(name);
    Ok(LoadedLayer { name: name.to_owned(), layer, unit, visible, color })
}

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Millimeters => "mm",
        Unit::Inches => "in",
    }
}

struct PcbApp {
    path_text: String,
    expert_mode: bool,
    bits: Vec<Bit>,
    selected_bit: usize,
    view_mode: ViewMode,
    log: Vec<String>,
    layers: Vec<LoadedLayer>,
    view: ViewState,
    ui_state: UiState,
    transform: GerberTransform,
    render_config: RenderConfiguration,
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
            log: vec!["Ready. Drop a Gerber file on the window.".into()],
            layers: Vec::new(),
            view: ViewState::default(),
            ui_state: UiState::default(),
            transform: GerberTransform::default(),
            render_config: RenderConfiguration::default(),
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
                        self.load_path(Path::new(&path));
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
                if ui
                    .checkbox(&mut self.transform.mirroring.x, "Mirror X")
                    .on_hover_text("Flip the layer, e.g. for bottom copper")
                    .changed()
                {
                    self.fit_requested = true;
                }

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

                // --- layers + log box
                boxed(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(ui.available_height() - 8.0);
                    ui.strong("Layers");
                    egui::ScrollArea::vertical()
                        .id_salt("layer_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for l in &mut self.layers {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut l.visible, "");
                                    ui.color_edit_button_srgba(&mut l.color);
                                    ui.label(&l.name);
                                });
                            }
                        });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("log")
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.log {
                                ui.label(line);
                            }
                        });
                });
            });
    }

    /// Centre: pannable / zoomable canvas, rendered by gerber_viewer
    fn canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;

            // fit to view: after loading, on double-click, or with F
            let fit_key = response.hovered() && ui.input(|i| i.key_pressed(egui::Key::F));
            if self.fit_requested || response.double_clicked() || fit_key {
                // fit the union of all visible layers, *after* the transform so
                // mirroring/rotation stay in view
                let mut total: Option<BoundingBox> = None;
                for l in self.layers.iter().filter(|l| l.visible) {
                    if let Some(b) = l.layer.try_bounding_box() {
                        let b = b.apply_transform(&self.transform);
                        match &mut total {
                            Some(t) => t.expand(&b),
                            None => total = Some(b),
                        }
                    }
                }
                if let Some(b) = total {
                    self.view.fit_view(rect, &b, 1.0);
                }
                self.fit_requested = false;
            }

            // pan / zoom / cursor tracking, all provided by gerber_viewer
            self.ui_state.update(ui, &rect, &response, &mut self.view);

            // origin axes
            let origin = self.ui_state.origin_screen_pos;
            let axis = Stroke::new(1.0, Color32::from_gray(90));
            painter.line_segment([Pos2::new(rect.left(), origin.y), Pos2::new(rect.right(), origin.y)], axis);
            painter.line_segment([Pos2::new(origin.x, rect.top()), Pos2::new(origin.x, rect.bottom())], axis);

            let unit = self
                .layers
                .iter()
                .find(|l| l.visible)
                .map(|l| unit_label(l.unit))
                .unwrap_or("mm");

            if self.layers.is_empty() {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Drop a Gerber file or .zip here\nor type its path above and press Enter",
                    FontId::proportional(16.0),
                    ui.visuals().weak_text_color(),
                );
            } else {
                for l in self.layers.iter().filter(|l| l.visible) {
                    let color = match self.view_mode {
                        ViewMode::Gerber => l.color,
                        ViewMode::Toolpath => Color32::from_gray(70),
                    };
                    GerberRenderer::new(&self.render_config, self.view, &self.transform, &l.layer)
                        .paint_layer(&painter, color);
                }
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

            // cursor position in gerber units
            if response.hovered() {
                if let Some(p) = self.ui_state.cursor_gerber_coords {
                    painter.text(
                        rect.left_bottom() + Vec2::new(8.0, -8.0),
                        Align2::LEFT_BOTTOM,
                        format!("x {:9.3} {unit}   y {:9.3} {unit}", p.x, p.y),
                        FontId::monospace(12.0),
                        ui.visuals().text_color(),
                    );
                }
            }
        });
    }

    fn handle_dropped_files(&mut self, ui: &egui::Ui) {
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let path = f.path();
            if path.is_file() {
                self.path_text = path.display().to_string();
                self.load_path(path);
            } else {
                match f.bytes() {
                    Ok(bytes) => self.load_single("dropped file", bytes),
                    Err(e) => self.log.push(format!("Could not read dropped file: {e}")),
                }
            }
        }
    }

    /// .zip -> all Gerber layers inside, anything else -> a single Gerber file
    fn load_path(&mut self, path: &Path) {
        let is_zip = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if is_zip {
            self.load_zip(path);
        } else {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    self.load_single(&name, bytes);
                }
                Err(e) => self.log.push(format!("Cannot read {}: {e}", path.display())),
            }
        }
    }

    fn load_single(&mut self, name: &str, bytes: Vec<u8>) {
        match parse_layer(name, bytes) {
            Ok(layer) => {
                self.log.push(format!("{name}: loaded"));
                self.layers = vec![layer];
                self.fit_requested = true;
            }
            Err(e) => self.log.push(format!("{name}: {e}")),
        }
    }

    fn load_zip(&mut self, path: &Path) {
        let Some(path_str) = path.to_str() else {
            self.log.push("Zip path is not valid UTF-8".into());
            return;
        };
        let files = match read_zip(path_str, ZIP_PATTERNS) {
            Ok(files) => files,
            Err(e) => {
                self.log.push(format!("Cannot read zip: {e}"));
                return;
            }
        };

        // HashMap order is random; sort so the layer list is stable
        let mut files: Vec<(String, Vec<u8>)> = files.into_iter().collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));

        let mut layers = Vec::new();
        for (entry_name, bytes) in files {
            // drop any folder prefix inside the zip
            let name = Path::new(&entry_name)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(entry_name);
            if name.to_ascii_lowercase().ends_with(".gbrjob") {
                continue; // KiCad job file: JSON, not a layer
            }
            match parse_layer(&name, bytes) {
                Ok(layer) => layers.push(layer),
                Err(e) => self.log.push(format!("{name}: skipped ({e})")),
            }
        }

        if layers.is_empty() {
            self.log.push("No Gerber layers found in zip".into());
            return;
        }
        self.log.push(format!("Loaded {} layers from zip", layers.len()));
        self.layers = layers;
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

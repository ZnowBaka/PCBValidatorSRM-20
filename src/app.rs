use std::path::Path;

use eframe::egui;
use egui_file_dialog::FileDialog;
use gerber_viewer::{GerberTransform, RenderConfiguration, UiState, ViewState};

use crate::board::layer::LoadedLayer;
use crate::board::loader::{self, LoadReport};
use crate::tools::bit::Bit;

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum ViewMode {
    Gerber,
    Toolpath,
}

pub struct PcbApp {
    pub(crate) path_text: String,
    pub(crate) expert_mode: bool,
    pub(crate) bits: Vec<Bit>,
    pub(crate) selected_bit: usize,
    pub(crate) view_mode: ViewMode,
    pub(crate) log: Vec<String>,
    pub(crate) layers: Vec<LoadedLayer>,
    pub(crate) view: ViewState,
    pub(crate) ui_state: UiState,
    pub(crate) transform: GerberTransform,
    pub(crate) render_config: RenderConfiguration,
    pub(crate) fit_requested: bool,
    pub(crate) file_dialog: FileDialog,
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
            file_dialog: FileDialog::new().add_file_filter_extensions(
                "Gerber and ZIP files",
                vec![
                    "gbr", "gtl", "gbl", "gts", "gbs", "gto", "gbo", "gm1", "gko", "gml", "zip",
                ],
            ),
        }
    }
}

impl PcbApp {
    fn handle_dropped_files(&mut self, ui: &egui::Ui) {
        let dropped = ui.input(|input| input.raw.dropped_files.clone());
        for file in dropped {
            let path = file.path();
            if path.is_file() {
                self.path_text = path.display().to_string();
                self.load_path(path);
            } else {
                match file.bytes() {
                    Ok(bytes) => self.load_single("dropped file", bytes),
                    Err(error) => self
                        .log
                        .push(format!("Could not read dropped file: {error}")),
                }
            }
        }
    }

    pub(crate) fn load_path(&mut self, path: &Path) {
        match loader::load_path(path) {
            Ok(report) => self.apply_load_report(report),
            Err(error) => self.log.push(error),
        }
    }

    fn load_single(&mut self, name: &str, bytes: Vec<u8>) {
        self.apply_load_report(loader::load_single(name, bytes));
    }

    fn apply_load_report(&mut self, report: LoadReport) {
        self.log.extend(report.messages);
        if !report.layers.is_empty() {
            self.layers = report.layers;
            self.fit_requested = true;
        }
    }

    pub(crate) fn open_file_browser(&mut self) {
        self.file_dialog.pick_file();
    }

    pub(crate) fn generate(&mut self) {
        let bit = &self.bits[self.selected_bit];
        self.log.push(format!(
            "Generate: bit '{}', {:.2} mm/s, {:.3} mm/pass",
            bit.name, bit.feedrate_mm_s, bit.depth_per_pass_mm
        ));
        // TODO: run toolpath generation (ideally on a background thread)
        self.view_mode = ViewMode::Toolpath;
    }
}

impl eframe::App for PcbApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui);
        crate::ui::show(self, ui);

        self.file_dialog.update(ui.ctx());
        if let Some(path) = self.file_dialog.take_picked() {
            self.path_text = path.display().to_string();
            self.load_path(&path);
        }
    }
}

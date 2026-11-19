use eframe::{App, NativeOptions};

struct GerberData {
    file_path: String,
    file_content: String
}

struct ValidatorUXApp;
impl App for ValidatorUXApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Validator UX");
            ui.label(&gerber_data.file_path);
            ui.label(&gerber_data.file_content);
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Validator UX",
        NativeOptions::default(),
        Box::new(|_creation_context| Ok(Box::new(ValidatorUXApp))),
    )
}

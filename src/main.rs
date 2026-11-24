use eframe::egui;


#[derive(Default)]
struct ValidatorUXApp{

}

impl eframe::App for ValidatorUXApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {

        egui::Frame::new().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new("PCB Validator")
                    .strong()
                    .size(30.0)
                    .color(egui::Color32::WHITE),
                    );
                ui.separator();
            });
        });

    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Validator UX",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(ValidatorUXApp::default())),)
    )
}

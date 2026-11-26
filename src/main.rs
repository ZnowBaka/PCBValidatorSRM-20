use eframe::egui;

use pcb_validator::app::PcbApp;

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

use eframe::egui::Color32;
use gerber_viewer::GerberLayer;
use gerber_viewer::gerber_types::Unit;

pub struct LoadedLayer {
    pub name: String,
    pub layer: GerberLayer,
    /// Coordinates in the layer are in this unit (gerber_viewer does not convert).
    pub unit: Unit,
    pub visible: bool,
    pub color: Color32,
    /// True when line breaks had to be restored for the parser.
    pub repaired: bool,
}

/// Guess a colour and default visibility from the file name (KiCad / Protel naming).
pub fn style_for(name: &str) -> (Color32, bool) {
    let n = name.to_ascii_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|key| n.contains(key));
    if has(&["edge_cuts", ".gm1", ".gko", ".gml", "outline", "profile"]) {
        (Color32::from_rgb(230, 220, 90), true)
    } else if has(&["silk", "mask", "paste", ".gto", ".gbo", ".gts", ".gbs"]) {
        (Color32::from_gray(150), false)
    } else if has(&["b_cu", ".gbl", "bottom"]) {
        (Color32::from_rgb(80, 140, 230), false)
    } else {
        (Color32::from_rgb(200, 150, 50), true)
    }
}

pub fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Millimeters => "mm",
        Unit::Inches => "in",
    }
}

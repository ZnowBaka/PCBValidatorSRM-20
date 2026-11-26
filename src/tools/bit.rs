#[derive(Clone)]
pub struct Bit {
    pub name: String,
    pub diameter_mm: f32,
    pub feedrate_mm_s: f32,
    pub depth_per_pass_mm: f32,
    pub plunge_rate_mm_s: f32,
    pub spindle_rpm: u32,
    pub stepover_pct: f32,
}

impl Bit {
    pub fn new(name: &str, diameter_mm: f32) -> Self {
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

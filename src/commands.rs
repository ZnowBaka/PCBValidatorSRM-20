#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps(pub i32); // 1 step = 0.01 mm

impl Steps {
    pub fn from_mm(mm: f64) -> Self {
        Steps((mm * 100.0).round() as i32)
    }
}

pub enum Cmd {
    Init,                                    // ^IN;
    ZHeights { cut: Steps, travel: Steps },  // !PZ cut,travel;
    SpeedXy(f64),                            // VS speed;
    SpeedZ(f64),                             // !VZ speed;
    Spindle(bool),                           // !MC 1 or 0;
    MoveTo(Steps, Steps),                    // PU x,y;
    CutTo(Steps, Steps),                     // PD x,y;
}

pub fn render(cmds: &[Cmd]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for c in cmds {
        match c {
            Cmd::Init => write!(out, "^IN;"),
            Cmd::ZHeights { cut, travel } => write!(out, "!PZ{},{};", cut.0, travel.0),
            Cmd::SpeedXy(v) => write!(out, "VS{v};"),
            Cmd::SpeedZ(v) => write!(out, "!VZ{v};"),
            Cmd::Spindle(on) => write!(out, "!MC{};", *on as u8),
            Cmd::MoveTo(x, y) => write!(out, "PU{},{};", x.0, y.0),
            Cmd::CutTo(x, y) => write!(out, "PD{},{};", x.0, y.0),
        }
        .unwrap();
        out.push('\n');
    }
    out
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps(pub i32); // 1 step = 0.01 mm

impl Steps {
    pub fn from_mm(mm: f64) -> Self {
        Steps((mm * 100.0).round() as i32)
    }
}

pub enum Cmd {
    Init, //sætter også spindal til true så med mindre spindal skal være låst til scribing er der ikke grund til at bruge spindel command
    ZHeights { cut: Steps, travel: Steps },  //sætter Z1 og Z2 vilket er den højde toolet skal skære
    SpeedXy(f64), //feedrate på tool i mm/s
    SpeedZ(f64), //op og ned hastighed på tool i mm/s
    Spindle(bool), //bestemmer om spindel må rotere
    MoveTo(Steps, Steps), //bevæger toolet til givet position med defineret Z2 højde
    CutTo(Steps, Steps), //samme som MoveTo men med Z1 value
    Dwell(u16), //toolet venter i den defineret til som er i millisekunder
    Stop, //stopper maskinen brug kun i nødstilfælde da maskinen kun kan startes igen via kontrolpanel
    SpindleRPM(u32), //sætter spindel rpm på maskinen som kan styres på 2 måder 0-15 er en range fra maskinens minimum hastighed til maximum hvor 0 er min og 15 er max derudover vil tal over 100 tætte en specifik rpm
    Home
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
            Cmd::Dwell(millis) => write!(out, "!DW{millis};"),
            Cmd::Stop => write!(out, "!NR;"),
            Cmd::SpindleRPM(rpm) => write!(out, "!RC{rpm}"),
            Cmd::Home => write!(out, "H;")
        }
        .unwrap();
        out.push('\n');
    }
    out
}
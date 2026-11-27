use anyhow::anyhow;
use gerber_parser::{GerberDoc, parse};
use gerber_types::{
    Aperture, Command, CoordinateNumber, CoordinateOffset, Coordinates, DCode, ExtendedCode,
    FunctionCode, GCode, InterpolationMode, Operation, Polarity,
};
use std::{collections::HashMap, io::BufReader};

use crate::zip_reader::read_zip;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: i64,
    pub y: i64,
}

pub enum Shape {
    Polygon(Vec<Position>),
    Stroke { path: Vec<Position>, width: i64 },
    Disk { center: Position, diameter: i64 },
}

pub struct Item {
    pub dark: bool,
    pub shape: Shape,
}

#[derive(Clone, Copy)]
enum Mode {
    Linear,
    Cw,
    Ccw,
}

struct State {
    current: Position,
    aperture: Option<i32>,
    mode: Mode,
    dark: bool,
    in_region: bool,
    contour: Vec<Position>,
    contours: Vec<Vec<Position>>,
    path: Vec<Position>,
    path_width: i64,
    items: Vec<Item>,
}

impl State {
    fn new() -> Self {
        State {
            current: Position { x: 0, y: 0 },
            aperture: None,
            mode: Mode::Linear,
            dark: true,
            in_region: false,
            contour: vec![],
            contours: vec![],
            path: vec![],
            path_width: 0,
            items: vec![],
        }
    }

    fn resolve(&self, coordinate: &Option<Coordinates>) -> Position {
        let mut position = self.current;
        if let Some(coordinate) = coordinate {
            if let Some(x) = &coordinate.x {
                position.x = coord(x)
            }
            if let Some(y) = &coordinate.y {
                position.y = coord(y)
            }
        }
        position
    }

    fn center(&self, offset: &Option<CoordinateOffset>) -> Position {
        let mut current = self.current;
        if let Some(offset) = offset {
            if let Some(i) = &offset.x {
                current.x = coord(i)
            }
            if let Some(j) = &offset.y {
                current.y = coord(j)
            }
        }
        current
    }

    fn flush_contour(&mut self) {
        if self.contour.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.contour));
        } else {
            self.contour.clear();
        }
    }

    fn emit_region(&mut self) {
        for c in std::mem::take(&mut self.contours) {
            self.items.push(Item {
                dark: self.dark,
                shape: Shape::Polygon(c),
            });
        }
    }

    fn flush_stroke(&mut self) {
        if self.path.len() >= 2 {
            let path = std::mem::take(&mut self.path);
            self.items.push(Item {
                dark: self.dark,
                shape: Shape::Stroke {
                    path,
                    width: self.path_width,
                },
            });
        } else {
            self.path.clear();
        }
    }

    fn circle_width(&self, aps: &HashMap<i32, Aperture>) -> Result<i64, String> {
        match self.aperture.and_then(|n| aps.get(&n)) {
            Some(Aperture::Circle(c)) => Ok((c.diameter * 1e6).round() as i64),
            Some(_) => Err("only circular apertures are supported so far".into()),
            None => Err("no aperture selected".into()),
        }
    }
}

pub fn interpret(doc: &GerberDoc) -> Result<Vec<Item>, String> {
    let mut state = State::new();
    for command in &doc.commands {
        let command = command.as_ref().map_err(|e| format!("{e:?}"))?;
        match command {
            Command::ExtendedCode(ExtendedCode::LoadPolarity(p)) => {
                state.flush_stroke();
                state.dark = matches!(p, Polarity::Dark);
            }
            Command::FunctionCode(FunctionCode::GCode(g)) => match g {
                GCode::InterpolationMode(m) => {
                    state.flush_stroke();
                    state.mode = match m {
                        InterpolationMode::Linear => Mode::Linear,
                        InterpolationMode::ClockwiseCircular => Mode::Cw,
                        InterpolationMode::CounterclockwiseCircular => Mode::Ccw,
                    };
                }
                GCode::RegionMode(true) => {
                    state.flush_stroke();
                    state.in_region = true;
                    state.contour.clear();
                    state.contours.clear();
                }
                GCode::RegionMode(false) => {
                    state.flush_contour();
                    state.emit_region();
                    state.in_region = false;
                }
                _ => {}
            },
            Command::FunctionCode(FunctionCode::DCode(d)) => match d {
                DCode::SelectAperture(n) => {
                    state.flush_stroke();
                    state.aperture = Some(*n);
                }
                DCode::Operation(Operation::Move(c)) => {
                    let to = state.resolve(c);
                    if state.in_region {
                        state.flush_contour(); // D02 inside a region starts a new contour
                        state.contour.push(to);
                    } else {
                        state.flush_stroke(); // D02 outside: lift the pen, the line ends
                    }
                    state.current = to;
                }
                DCode::Operation(Operation::Interpolate(c, offset)) => {
                    let end = state.resolve(c);
                    let pts = match state.mode {
                        Mode::Linear => vec![end],
                        _ => {
                            flatten_arc(state.current, end, state.center(offset), state.mode, 5_000)
                        }
                    };
                    if state.in_region {
                        state.contour.extend(pts);
                    } else {
                        if state.path.is_empty() {
                            state.path_width = state.circle_width(&doc.apertures)?;
                            state.path.push(state.current); // the stroke starts where the pen was
                        }
                        state.path.extend(pts);
                    }
                    state.current = end;
                }
                DCode::Operation(Operation::Flash(c)) => {
                    state.flush_stroke();
                    state.current = state.resolve(c);
                    let diameter = state.circle_width(&doc.apertures)?;
                    state.items.push(Item {
                        dark: state.dark,
                        shape: Shape::Disk {
                            center: state.current,
                            diameter,
                        },
                    });
                }
                /*_ => {}*/
            },
            Command::FunctionCode(FunctionCode::MCode(_)) => state.flush_stroke(), // end of file
            _ => {}
        }
    }
    state.flush_stroke();
    Ok(state.items)
}

fn flatten_arc(
    start: Position,
    end: Position,
    center: Position,
    mode: Mode,
    tol: i64,
) -> Vec<Position> {
    let (cx, cy) = (center.x as f64, center.y as f64);
    let r = ((start.x as f64 - cx).hypot(start.y as f64 - cy)).max(1.0);

    let a0 = (start.y as f64 - cy).atan2(start.x as f64 - cx);
    let mut a1 = (end.y as f64 - cy).atan2(end.x as f64 - cx);

    let tau = std::f64::consts::TAU;
    match mode {
        Mode::Ccw => {
            while a1 <= a0 {
                a1 += tau;
            }
        }
        Mode::Cw => {
            while a1 >= a0 {
                a1 -= tau;
            }
        }
        Mode::Linear => unreachable!(),
    }

    let step = 2.0 * (1.0 - tol as f64 / r).clamp(-1.0, 1.0).acos();
    let n = ((a1 - a0).abs() / step).ceil().max(1.0) as usize;

    (1..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f64 / n as f64;
            Position {
                x: (cx + r * a.cos()).round() as i64,
                y: (cy + r * a.sin()).round() as i64,
            }
        })
        .collect()
}

fn coord(n: &CoordinateNumber) -> i64 {
    const SCALE: f64 = 1_000_000.0;
    let v: f64 = (*n).into();
    (v * SCALE).round() as i64
}

pub fn grb_parse(zip_path: String) -> anyhow::Result<Vec<GerberDoc>> {
    let file_names: Vec<&str> = vec![
        "copper_top.gbr",
        "soldermask_top.gbr",
        "silkscreen_top.gbr",
        "profile.gbr",
    ];

    let _some_input = String::from(zip_path);
    let gerber_files = read_zip(&_some_input, &file_names)?;
    let mut docs = Vec::new();

    for (name, bytes) in &gerber_files {
        let reader = BufReader::new(bytes.as_slice());
        let doc = parse(reader)
            .map_err(|(_partial_doc, e)| anyhow!("failed to parse: {name} : {:?}", e))?;

        docs.push(doc);
    }
    Ok(docs)
}
// svg.rs - debug export of interpreted Gerber layers to SVG.
//
// Expects these types at the crate root (adjust the `use` line if yours live elsewhere):
//   struct Pt { x: i64, y: i64 }                      // 1 unit = 1e-6 mm
//   enum Shape { Polygon(Vec<Pt>), Stroke { path: Vec<Pt>, width: i64 }, Disk { center: Pt, diameter: i64 } }
//   struct Item { dark: bool, shape: Shape }
use crate::parser::gerber::{Item, Shape};
use std::fmt::Write as _;

const UNIT: f64 = 1_000_000.0; // integer units per mm
const MARGIN_MM: f64 = 1.0;
const PX_PER_MM: f64 = 10.0;

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Bounds { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY }
    }

    // grow the box to contain a point, padded by `pad` on every side
    fn include(&mut self, x: f64, y: f64, pad: f64) {
        self.min_x = self.min_x.min(x - pad);
        self.min_y = self.min_y.min(y - pad);
        self.max_x = self.max_x.max(x + pad);
        self.max_y = self.max_y.max(y + pad);
    }
}

fn mm(v: i64) -> f64 {
    v as f64 / UNIT
}

fn bounds(layers: &[(&str, &[Item])]) -> Bounds {
    let mut b = Bounds::empty();
    for (_, items) in layers {
        for item in items.iter() {
            match &item.shape {
                Shape::Polygon(pts) => {
                    for p in pts {
                        b.include(mm(p.x), mm(p.y), 0.0);
                    }
                }
                Shape::Stroke { path, width } => {
                    for p in path {
                        b.include(mm(p.x), mm(p.y), mm(*width) / 2.0);
                    }
                }
                Shape::Disk { center, diameter } => {
                    b.include(mm(center.x), mm(center.y), mm(*diameter) / 2.0);
                }
            }
        }
    }
    if b.min_x > b.max_x {
        // nothing to draw: give the SVG a harmless 1x1 mm box
        b = Bounds { min_x: 0.0, min_y: 0.0, max_x: 1.0, max_y: 1.0 };
    }
    b
}

// Draw one shape in the given color. `flip` converts a Gerber Y (up) to an SVG Y (down).
fn write_shape(out: &mut String, shape: &Shape, color: &str, flip: &dyn Fn(f64) -> f64) {
    match shape {
        Shape::Polygon(pts) => {
            let mut d = String::new();
            for (i, p) in pts.iter().enumerate() {
                let cmd = if i == 0 { 'M' } else { 'L' };
                write!(d, "{cmd}{:.4},{:.4} ", mm(p.x), flip(mm(p.y))).unwrap();
            }
            d.push('Z');
            writeln!(out, r#"<path d="{d}" fill="{color}"/>"#).unwrap();
        }
        Shape::Stroke { path, width } => {
            let mut points = String::new();
            for p in path {
                write!(points, "{:.4},{:.4} ", mm(p.x), flip(mm(p.y))).unwrap();
            }
            writeln!(
                out,
                r#"<polyline points="{points}" fill="none" stroke="{color}" stroke-width="{:.4}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                mm(*width)
            )
            .unwrap();
        }
        Shape::Disk { center, diameter } => {
            writeln!(
                out,
                r#"<circle cx="{:.4}" cy="{:.4}" r="{:.4}" fill="{color}"/>"#,
                mm(center.x),
                flip(mm(center.y)),
                mm(*diameter) / 2.0
            )
            .unwrap();
        }
    }
}

/// Render several layers on top of each other. Each layer is (css color, items).
/// Items must be in drawing order, because a clear shape only erases what came before it
/// *within the same layer*. Each layer is built with its own SVG mask so clear shapes
/// never erase other layers.
pub fn layers_to_svg(layers: &[(&str, &[Item])]) -> String {
    let b = bounds(layers);
    let (vx, vy) = (b.min_x - MARGIN_MM, b.min_y - MARGIN_MM);
    let (vw, vh) = (b.max_x - b.min_x + 2.0 * MARGIN_MM, b.max_y - b.min_y + 2.0 * MARGIN_MM);
    let flip = |y: f64| b.max_y + b.min_y - y;

    let mut s = String::new();
    writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vx:.4} {vy:.4} {vw:.4} {vh:.4}" width="{:.0}" height="{:.0}">"#,
        vw * PX_PER_MM,
        vh * PX_PER_MM
    )
    .unwrap();
    writeln!(s, r##"<rect x="{vx:.4}" y="{vy:.4}" width="{vw:.4}" height="{vh:.4}" fill="#ffffff"/>"##).unwrap();

    for (i, (color, items)) in layers.iter().enumerate() {
        // mask: starts black (nothing), dark shapes paint white, clear shapes paint black
        writeln!(
            s,
            r#"<mask id="m{i}" maskUnits="userSpaceOnUse" x="{vx:.4}" y="{vy:.4}" width="{vw:.4}" height="{vh:.4}">"#
        )
        .unwrap();
        writeln!(s, r##"<rect x="{vx:.4}" y="{vy:.4}" width="{vw:.4}" height="{vh:.4}" fill="#000000"/>"##).unwrap();
        for item in items.iter() {
            let paint = if item.dark { "#ffffff" } else { "#000000" };
            write_shape(&mut s, &item.shape, paint, &flip);
        }
        writeln!(s, "</mask>").unwrap();

        // the layer color, visible only where the mask is white
        writeln!(
            s,
            r#"<rect x="{vx:.4}" y="{vy:.4}" width="{vw:.4}" height="{vh:.4}" fill="{color}" mask="url(#m{i})"/>"#
        )
        .unwrap();
    }

    writeln!(s, "</svg>").unwrap();
    s
}

pub fn write_svg(path: &str, layers: &[(&str, &[Item])]) -> std::io::Result<()> {
    std::fs::write(path, layers_to_svg(layers))
}
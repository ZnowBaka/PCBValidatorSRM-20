//! Minimal RS-274X (Gerber) parser + tessellator. No GUI dependencies.
//!
//! Supports: FS/MO, AD (C, R, O, P, macros), AM (primitives 1, 4, 5, 20, 21, 22
//! with variables and arithmetic), D01/D02/D03, G01/G02/G03, G74/G75,
//! G36/G37 regions, LP polarity, SR step-and-repeat, G70/G71.
//!
//! Output is a triangle mesh in millimetres (Y up) that a renderer can draw
//! directly. Clear polarity is flagged per vertex so the renderer can paint it
//! with the background colour.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

type P = [f64; 2];

/// Max deviation of curves from the true shape, in mm.
const TOLERANCE_MM: f64 = 0.003;

#[derive(Clone, Copy, Debug)]
pub struct Vert {
    pub pos: [f32; 2],
    pub clear: bool,
}

#[derive(Default, Debug)]
pub struct GerberLayer {
    pub verts: Vec<Vert>,
    pub indices: Vec<u32>,
    /// (min, max) in mm
    pub bounds: Option<([f32; 2], [f32; 2])>,
    pub warnings: Vec<String>,
    pub n_draws: usize,
    pub n_flashes: usize,
    pub n_regions: usize,
}

// ------------------------------------------------------------ primitives ----

#[derive(Clone, Debug)]
enum Prim {
    Disc { c: P, r: f64 },
    Poly { pts: Vec<P> },
    Stroke { pts: Vec<P>, width: f64 },
}

#[derive(Clone, Debug)]
struct Item {
    prim: Prim,
    dark: bool,
}

impl Prim {
    fn map(&self, f: impl Fn(P) -> P) -> Prim {
        match self {
            Prim::Disc { c, r } => Prim::Disc { c: f(*c), r: *r },
            Prim::Poly { pts } => Prim::Poly { pts: pts.iter().map(|p| f(*p)).collect() },
            Prim::Stroke { pts, width } => Prim::Stroke {
                pts: pts.iter().map(|p| f(*p)).collect(),
                width: *width,
            },
        }
    }
    fn translated(&self, dx: f64, dy: f64) -> Prim {
        self.map(|p| [p[0] + dx, p[1] + dy])
    }
    fn scaled(&self, s: f64) -> Prim {
        let m = self.map(|p| [p[0] * s, p[1] * s]);
        match m {
            Prim::Disc { c, r } => Prim::Disc { c, r: r * s },
            Prim::Stroke { pts, width } => Prim::Stroke { pts, width: width * s },
            other => other,
        }
    }
}

// -------------------------------------------------------------- apertures ----

#[derive(Clone, Debug)]
enum Aperture {
    Circle { d: f64, hole: f64 },
    Rect { w: f64, h: f64, hole: f64 },
    Obround { w: f64, h: f64, hole: f64 },
    Polygon { d: f64, n: u32, rot: f64, hole: f64 },
    Macro { name: String, params: Vec<f64>, scale: f64 },
}

impl Aperture {
    /// Approximate stroking width when this aperture is used with D01.
    fn stroke_width(&self) -> f64 {
        match self {
            Aperture::Circle { d, .. } => *d,
            Aperture::Rect { w, h, .. } | Aperture::Obround { w, h, .. } => w.min(*h),
            Aperture::Polygon { d, .. } => *d,
            Aperture::Macro { .. } => 0.01,
        }
    }
}

fn regular_polygon(c: P, d: f64, n: u32, rot_deg: f64) -> Vec<P> {
    let n = n.max(3);
    let r = d / 2.0;
    (0..n)
        .map(|k| {
            let a = rot_deg.to_radians() + TAU * k as f64 / n as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

fn rect_poly(cx: f64, cy: f64, w: f64, h: f64) -> Vec<P> {
    let (hw, hh) = (w / 2.0, h / 2.0);
    vec![[cx - hw, cy - hh], [cx + hw, cy - hh], [cx + hw, cy + hh], [cx - hw, cy + hh]]
}

fn rotate(p: P, deg: f64) -> P {
    if deg == 0.0 {
        return p;
    }
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn flash(ap: &Aperture, at: P, dark: bool, macros: &HashMap<String, Vec<String>>) -> Vec<Item> {
    let mut out = Vec::new();
    let mut push = |prim: Prim, dark: bool| out.push(Item { prim, dark });
    match ap {
        Aperture::Circle { d, hole } => {
            push(Prim::Disc { c: at, r: d / 2.0 }, dark);
            if *hole > 0.0 && dark {
                push(Prim::Disc { c: at, r: hole / 2.0 }, false);
            }
        }
        Aperture::Rect { w, h, hole } => {
            push(Prim::Poly { pts: rect_poly(at[0], at[1], *w, *h) }, dark);
            if *hole > 0.0 && dark {
                push(Prim::Disc { c: at, r: hole / 2.0 }, false);
            }
        }
        Aperture::Obround { w, h, hole } => {
            if (w - h).abs() < 1e-9 {
                push(Prim::Disc { c: at, r: w / 2.0 }, dark);
            } else if w > h {
                let o = (w - h) / 2.0;
                push(
                    Prim::Stroke { pts: vec![[at[0] - o, at[1]], [at[0] + o, at[1]]], width: *h },
                    dark,
                );
            } else {
                let o = (h - w) / 2.0;
                push(
                    Prim::Stroke { pts: vec![[at[0], at[1] - o], [at[0], at[1] + o]], width: *w },
                    dark,
                );
            }
            if *hole > 0.0 && dark {
                push(Prim::Disc { c: at, r: hole / 2.0 }, false);
            }
        }
        Aperture::Polygon { d, n, rot, hole } => {
            push(Prim::Poly { pts: regular_polygon(at, *d, *n, *rot) }, dark);
            if *hole > 0.0 && dark {
                push(Prim::Disc { c: at, r: hole / 2.0 }, false);
            }
        }
        Aperture::Macro { name, params, scale } => {
            if let Some(blocks) = macros.get(name) {
                for (prim, exposure) in expand_macro(blocks, params) {
                    // exposure off inside a dark flash => clear; inside a clear flash stays clear
                    let d = dark && exposure;
                    push(prim.scaled(*scale).translated(at[0], at[1]), d);
                }
            }
        }
    }
    out
}

// ------------------------------------------------------- macro expressions ----

struct Ev<'a> {
    s: &'a [u8],
    i: usize,
    vars: &'a HashMap<usize, f64>,
}

impl<'a> Ev<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> f64 {
        let mut v = self.term();
        while let Some(c) = self.peek() {
            match c {
                b'+' => {
                    self.i += 1;
                    v += self.term();
                }
                b'-' => {
                    self.i += 1;
                    v -= self.term();
                }
                _ => break,
            }
        }
        v
    }
    fn term(&mut self) -> f64 {
        let mut v = self.factor();
        while let Some(c) = self.peek() {
            match c {
                b'x' | b'X' | b'*' => {
                    self.i += 1;
                    v *= self.factor();
                }
                b'/' => {
                    self.i += 1;
                    let d = self.factor();
                    v = if d != 0.0 { v / d } else { 0.0 };
                }
                _ => break,
            }
        }
        v
    }
    fn factor(&mut self) -> f64 {
        match self.peek() {
            Some(b'-') => {
                self.i += 1;
                -self.factor()
            }
            Some(b'+') => {
                self.i += 1;
                self.factor()
            }
            Some(b'(') => {
                self.i += 1;
                let v = self.expr();
                if self.peek() == Some(b')') {
                    self.i += 1;
                }
                v
            }
            Some(b'$') => {
                self.i += 1;
                let start = self.i;
                while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                let n: usize = std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                *self.vars.get(&n).unwrap_or(&0.0)
            }
            _ => {
                let start = self.i;
                while self.peek().map_or(false, |c| c.is_ascii_digit() || c == b'.') {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .unwrap_or_else(|| {
                        // avoid infinite loops on garbage
                        if self.i == start {
                            self.i += 1;
                        }
                        0.0
                    })
            }
        }
    }
}

fn eval(expr: &str, vars: &HashMap<usize, f64>) -> f64 {
    let cleaned: Vec<u8> = expr.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut ev = Ev { s: &cleaned, i: 0, vars };
    ev.expr()
}

/// Expands a macro into primitives in file units, local coordinates.
/// The bool is "exposure on".
fn expand_macro(blocks: &[String], params: &[f64]) -> Vec<(Prim, bool)> {
    let mut vars: HashMap<usize, f64> = HashMap::new();
    for (i, p) in params.iter().enumerate() {
        vars.insert(i + 1, *p);
    }
    let mut out = Vec::new();

    for blk in blocks {
        let blk = blk.trim();
        // code 0 is a comment
        if blk.is_empty() || blk.starts_with('0') {
            continue;
        }
        if let Some(rest) = blk.strip_prefix('$') {
            if let Some((lhs, rhs)) = rest.split_once('=') {
                if let Ok(n) = lhs.trim().parse::<usize>() {
                    let v = eval(rhs, &vars);
                    vars.insert(n, v);
                }
            }
            continue;
        }
        let fields: Vec<&str> = blk.split(',').collect();
        let code: i32 = match fields[0].trim().parse() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let a: Vec<f64> = fields[1..].iter().map(|f| eval(f, &vars)).collect();
        let get = |i: usize| a.get(i).copied().unwrap_or(0.0);

        match code {
            1 => {
                // exposure, diameter, cx, cy, [rot]
                let c = rotate([get(2), get(3)], get(4));
                out.push((Prim::Disc { c, r: get(1) / 2.0 }, get(0) != 0.0));
            }
            20 => {
                // exposure, width, x1,y1, x2,y2, rot
                let (w, p1, p2, rot) = (get(1), [get(2), get(3)], [get(4), get(5)], get(6));
                let (dx, dy) = (p2[0] - p1[0], p2[1] - p1[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    let n = [-dy / len * w / 2.0, dx / len * w / 2.0];
                    let pts = vec![
                        [p1[0] + n[0], p1[1] + n[1]],
                        [p2[0] + n[0], p2[1] + n[1]],
                        [p2[0] - n[0], p2[1] - n[1]],
                        [p1[0] - n[0], p1[1] - n[1]],
                    ];
                    out.push((
                        Prim::Poly { pts: pts.into_iter().map(|p| rotate(p, rot)).collect() },
                        get(0) != 0.0,
                    ));
                }
            }
            21 => {
                // exposure, w, h, cx, cy, rot
                let pts = rect_poly(get(3), get(4), get(1), get(2));
                out.push((
                    Prim::Poly { pts: pts.into_iter().map(|p| rotate(p, get(5))).collect() },
                    get(0) != 0.0,
                ));
            }
            22 => {
                // exposure, w, h, x_lower_left, y_lower_left, rot
                let (w, h, x, y) = (get(1), get(2), get(3), get(4));
                let pts = rect_poly(x + w / 2.0, y + h / 2.0, w, h);
                out.push((
                    Prim::Poly { pts: pts.into_iter().map(|p| rotate(p, get(5))).collect() },
                    get(0) != 0.0,
                ));
            }
            4 => {
                // exposure, n, x0,y0, ..., xn,yn, rot
                let n = get(1) as usize;
                let mut pts = Vec::new();
                for k in 0..=n {
                    pts.push([get(2 + 2 * k), get(3 + 2 * k)]);
                }
                let rot = get(4 + 2 * n);
                out.push((
                    Prim::Poly { pts: pts.into_iter().map(|p| rotate(p, rot)).collect() },
                    get(0) != 0.0,
                ));
            }
            5 => {
                // exposure, nverts, cx, cy, diameter, rot
                // (rotation applies around the macro origin, not the polygon centre)
                let c = [get(2), get(3)];
                let rot = get(5);
                let pts: Vec<P> = regular_polygon([0.0, 0.0], get(4), get(1) as u32, 0.0)
                    .into_iter()
                    .map(|p| rotate([p[0] + c[0], p[1] + c[1]], rot))
                    .collect();
                out.push((Prim::Poly { pts }, get(0) != 0.0));
            }
            _ => { /* 6 (moire), 7 (thermal): unsupported, skipped */ }
        }
    }
    out
}

// ------------------------------------------------------------ tessellation ----

struct MeshBuilder {
    verts: Vec<Vert>,
    indices: Vec<u32>,
    min: [f32; 2],
    max: [f32; 2],
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            verts: Vec::new(),
            indices: Vec::new(),
            min: [f32::INFINITY; 2],
            max: [f32::NEG_INFINITY; 2],
        }
    }

    fn vert(&mut self, p: P, clear: bool) -> u32 {
        let pos = [p[0] as f32, p[1] as f32];
        for k in 0..2 {
            self.min[k] = self.min[k].min(pos[k]);
            self.max[k] = self.max[k].max(pos[k]);
        }
        self.verts.push(Vert { pos, clear });
        (self.verts.len() - 1) as u32
    }

    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    fn disc(&mut self, c: P, r: f64, clear: bool) {
        if r <= 0.0 {
            return;
        }
        let n = disc_segments(r);
        let centre = self.vert(c, clear);
        let first = self.verts.len() as u32;
        for k in 0..n {
            let a = TAU * k as f64 / n as f64;
            self.vert([c[0] + r * a.cos(), c[1] + r * a.sin()], clear);
        }
        for k in 0..n as u32 {
            self.tri(centre, first + k, first + (k + 1) % n as u32);
        }
    }

    fn poly(&mut self, pts: &[P], clear: bool) {
        let pts = clean_ring(pts);
        if pts.len() < 3 {
            return;
        }
        let base = self.verts.len() as u32;
        for p in &pts {
            self.vert(*p, clear);
        }
        for [a, b, c] in triangulate(&pts) {
            self.tri(base + a as u32, base + b as u32, base + c as u32);
        }
    }

    fn stroke(&mut self, pts: &[P], width: f64, clear: bool) {
        let mut v: Vec<P> = Vec::with_capacity(pts.len());
        for p in pts {
            if v.last().map_or(true, |q| (q[0] - p[0]).abs() > 1e-9 || (q[1] - p[1]).abs() > 1e-9) {
                v.push(*p);
            }
        }
        if v.is_empty() {
            return;
        }
        let r = width / 2.0;
        if v.len() == 1 {
            self.disc(v[0], r, clear);
            return;
        }
        for w in v.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            let n = [-dy / len * r, dx / len * r];
            let i0 = self.vert([a[0] + n[0], a[1] + n[1]], clear);
            let i1 = self.vert([b[0] + n[0], b[1] + n[1]], clear);
            let i2 = self.vert([b[0] - n[0], b[1] - n[1]], clear);
            let i3 = self.vert([a[0] - n[0], a[1] - n[1]], clear);
            self.tri(i0, i1, i2);
            self.tri(i0, i2, i3);
        }
        // round caps + joins (joins only where the turn is noticeable)
        self.disc(v[0], r, clear);
        self.disc(*v.last().unwrap(), r, clear);
        for k in 1..v.len() - 1 {
            let d1 = [v[k][0] - v[k - 1][0], v[k][1] - v[k - 1][1]];
            let d2 = [v[k + 1][0] - v[k][0], v[k + 1][1] - v[k][1]];
            let cross = d1[0] * d2[1] - d1[1] * d2[0];
            let dot = d1[0] * d2[0] + d1[1] * d2[1];
            if cross.atan2(dot).abs() > 0.1 {
                self.disc(v[k], r, clear);
            }
        }
    }
}

fn disc_segments(r: f64) -> usize {
    if r <= TOLERANCE_MM {
        return 8;
    }
    let step = 2.0 * (1.0 - TOLERANCE_MM / r).acos();
    ((TAU / step).ceil() as usize).clamp(8, 96)
}

/// Drops consecutive duplicates and a closing point equal to the first.
fn clean_ring(pts: &[P]) -> Vec<P> {
    let mut v: Vec<P> = Vec::with_capacity(pts.len());
    for p in pts {
        if v.last().map_or(true, |q| (q[0] - p[0]).abs() > 1e-9 || (q[1] - p[1]).abs() > 1e-9) {
            v.push(*p);
        }
    }
    while v.len() > 1 {
        let (f, l) = (v[0], *v.last().unwrap());
        if (f[0] - l[0]).abs() < 1e-9 && (f[1] - l[1]).abs() < 1e-9 {
            v.pop();
        } else {
            break;
        }
    }
    v
}

fn cross3(a: P, b: P, c: P) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Ear clipping for a simple polygon (concave allowed). Returns index triples.
fn triangulate(pts: &[P]) -> Vec<[usize; 3]> {
    let n = pts.len();
    let mut tris = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 {
        return tris;
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let mut idx: Vec<usize> = (0..n).collect();
    if area < 0.0 {
        idx.reverse();
    }
    const EPS: f64 = 1e-12;

    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            let cr = cross3(a, b, c);
            if cr.abs() <= EPS {
                idx.remove(k); // degenerate / collinear vertex
                clipped = true;
                break;
            }
            if cr < 0.0 {
                continue; // reflex
            }
            let blocked = idx.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let p = pts[j];
                // Coincident points (keyhole cut-ins) never block their own ear.
                let same = |q: P| (q[0] - p[0]).abs() < 1e-9 && (q[1] - p[1]).abs() < 1e-9;
                if same(a) || same(b) || same(c) {
                    return false;
                }
                // Inclusive test: a point on the ear's edge also blocks it.
                cross3(a, b, p) >= -EPS && cross3(b, c, p) >= -EPS && cross3(c, a, p) >= -EPS
            });
            if !blocked {
                tris.push([ia, ib, ic]);
                idx.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Self-intersecting or otherwise broken: fall back to a fan.
            for k in 1..idx.len() - 1 {
                tris.push([idx[0], idx[k], idx[k + 1]]);
            }
            return tris;
        }
    }
    if idx.len() == 3 && cross3(pts[idx[0]], pts[idx[1]], pts[idx[2]]).abs() > EPS {
        tris.push([idx[0], idx[1], idx[2]]);
    }
    tris
}

// ----------------------------------------------------------------- parser ----

#[derive(Clone, Copy, PartialEq)]
enum Interp {
    Linear,
    Cw,
    Ccw,
}

struct SrState {
    start: usize,
    nx: u32,
    ny: u32,
    dx: f64,
    dy: f64,
}

struct Parser {
    // format
    int_digits: usize,
    dec_digits: usize,
    trailing_zero_omit: bool,
    scale: f64, // file unit -> mm
    // graphics state
    pos: P,
    interp: Interp,
    multi_quadrant: bool,
    cur_ap: Option<i32>,
    dark: bool,
    last_op: i32,
    // regions
    in_region: bool,
    contour: Vec<P>,
    // strokes
    stroke: Vec<P>,
    // tables
    apertures: HashMap<i32, Aperture>,
    macros: HashMap<String, Vec<String>>,
    // output
    items: Vec<Item>,
    sr: Option<SrState>,
    out: GerberLayer,
    warned: std::collections::HashSet<String>,
}

impl Parser {
    fn new() -> Self {
        Self {
            int_digits: 2,
            dec_digits: 4,
            trailing_zero_omit: false,
            scale: 1.0,
            pos: [0.0, 0.0],
            interp: Interp::Linear,
            multi_quadrant: true,
            cur_ap: None,
            dark: true,
            last_op: 2,
            in_region: false,
            contour: Vec::new(),
            stroke: Vec::new(),
            apertures: HashMap::new(),
            macros: HashMap::new(),
            items: Vec::new(),
            sr: None,
            out: GerberLayer::default(),
            warned: Default::default(),
        }
    }

    fn warn(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if self.warned.insert(m.clone()) {
            self.out.warnings.push(m);
        }
    }

    fn coord(&self, raw: &str) -> f64 {
        if raw.contains('.') {
            return raw.parse::<f64>().unwrap_or(0.0) * self.scale;
        }
        let (neg, digits) = match raw.as_bytes().first() {
            Some(b'-') => (true, &raw[1..]),
            Some(b'+') => (false, &raw[1..]),
            _ => (false, raw),
        };
        let mut d = digits.to_string();
        if self.trailing_zero_omit {
            while d.len() < self.int_digits + self.dec_digits {
                d.push('0');
            }
        }
        let v = d.parse::<f64>().unwrap_or(0.0) / 10f64.powi(self.dec_digits as i32);
        (if neg { -v } else { v }) * self.scale
    }

    // ---- extended commands (%...%)
    fn extended(&mut self, body: &str) {
        let body = body.trim();
        if body.starts_with("AM") {
            let mut parts = body.split('*');
            let name = parts.next().unwrap_or("")[2..].trim().to_string();
            let blocks: Vec<String> = parts
                .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
                .filter(|s| !s.is_empty())
                .collect();
            self.macros.insert(name, blocks);
            return;
        }
        for cmd in body.split('*') {
            let cmd: String = cmd.chars().filter(|c| !c.is_whitespace()).collect();
            if cmd.is_empty() {
                continue;
            }
            self.ext_cmd(&cmd);
        }
    }

    fn ext_cmd(&mut self, cmd: &str) {
        if let Some(f) = cmd.strip_prefix("FS") {
            self.trailing_zero_omit = f.starts_with('T');
            if f.contains('I') && !f.contains("LAI") {
                // incremental coordinates are not supported
            }
            if f.contains("AI") || f.contains("LI") || f.contains("TI") {
                self.warn("Incremental coordinates (FS..I) are not supported");
            }
            if let Some(xp) = f.find('X') {
                let d: Vec<char> = f[xp + 1..].chars().take(2).collect();
                if d.len() == 2 {
                    self.int_digits = d[0].to_digit(10).unwrap_or(2) as usize;
                    self.dec_digits = d[1].to_digit(10).unwrap_or(4) as usize;
                }
            }
        } else if let Some(u) = cmd.strip_prefix("MO") {
            self.scale = if u.starts_with("IN") { 25.4 } else { 1.0 };
        } else if let Some(rest) = cmd.strip_prefix("AD") {
            self.define_aperture(rest);
        } else if let Some(p) = cmd.strip_prefix("LP") {
            self.flush_stroke();
            self.dark = p.starts_with('D');
        } else if let Some(rest) = cmd.strip_prefix("SR") {
            self.step_repeat(rest);
        } else if cmd.starts_with("LR") || cmd.starts_with("LM") || cmd.starts_with("LS") {
            self.warn("Aperture transformations (LR/LM/LS) are ignored");
        } else if cmd.starts_with("IPNEG") {
            self.warn("Negative image polarity (IPNEG) is not supported");
        }
        // TF/TA/TO/TD/LN/IP/OF/SF/AS/MI etc.: ignored
    }

    fn define_aperture(&mut self, rest: &str) {
        let rest = match rest.strip_prefix('D') {
            Some(r) => r,
            None => return,
        };
        let dlen = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        let code: i32 = match rest[..dlen].parse() {
            Ok(c) => c,
            Err(_) => return,
        };
        let tpl = &rest[dlen..];
        let (name, params) = tpl.split_once(',').unwrap_or((tpl, ""));
        let vals: Vec<f64> = params
            .split(|c| c == 'X' || c == 'x')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<f64>().unwrap_or(0.0))
            .collect();
        let g = |i: usize| vals.get(i).copied().unwrap_or(0.0);
        let s = self.scale;
        let ap = match name {
            "C" => Aperture::Circle { d: g(0) * s, hole: g(1) * s },
            "R" => Aperture::Rect { w: g(0) * s, h: g(1) * s, hole: g(2) * s },
            "O" => Aperture::Obround { w: g(0) * s, h: g(1) * s, hole: g(2) * s },
            "P" => Aperture::Polygon {
                d: g(0) * s,
                n: g(1) as u32,
                rot: g(2),
                hole: g(3) * s,
            },
            other => Aperture::Macro { name: other.to_string(), params: vals, scale: s },
        };
        if let Aperture::Macro { name, .. } = &ap {
            if !self.macros.contains_key(name) {
                let m = format!("Aperture D{code} uses undefined macro '{name}'");
                self.warn(m);
            }
        }
        self.apertures.insert(code, ap);
    }

    fn step_repeat(&mut self, rest: &str) {
        self.flush_stroke();
        self.close_sr();
        if rest.is_empty() {
            return;
        }
        let (mut nx, mut ny, mut dx, mut dy) = (1u32, 1u32, 0.0, 0.0);
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i] as char;
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.' || b[j] == b'-' || b[j] == b'+') {
                j += 1;
            }
            let v = &rest[start..j];
            match c {
                'X' => nx = v.parse().unwrap_or(1),
                'Y' => ny = v.parse().unwrap_or(1),
                'I' => dx = v.parse::<f64>().unwrap_or(0.0) * self.scale,
                'J' => dy = v.parse::<f64>().unwrap_or(0.0) * self.scale,
                _ => {}
            }
            i = j.max(i + 1);
        }
        self.sr = Some(SrState { start: self.items.len(), nx, ny, dx, dy });
    }

    fn close_sr(&mut self) {
        if let Some(sr) = self.sr.take() {
            let block: Vec<Item> = self.items[sr.start..].to_vec();
            for ix in 0..sr.nx {
                for iy in 0..sr.ny {
                    if ix == 0 && iy == 0 {
                        continue;
                    }
                    let (ox, oy) = (ix as f64 * sr.dx, iy as f64 * sr.dy);
                    for it in &block {
                        self.items.push(Item { prim: it.prim.translated(ox, oy), dark: it.dark });
                    }
                }
            }
        }
    }

    // ---- function-code blocks
    fn block(&mut self, blk: &str) {
        let blk: String = blk.chars().filter(|c| !c.is_whitespace()).collect();
        if blk.is_empty() || blk.starts_with("G04") || blk.starts_with("M02") || blk.starts_with("M00") {
            return;
        }

        let mut gs: Vec<i32> = Vec::new();
        let (mut x, mut y, mut i, mut j): (Option<f64>, Option<f64>, Option<f64>, Option<f64>) =
            (None, None, None, None);
        let mut d: Option<i32> = None;

        let b = blk.as_bytes();
        let mut k = 0;
        while k < b.len() {
            let c = b[k] as char;
            if "GXYIJDM".contains(c) {
                let start = k + 1;
                let mut e = start;
                while e < b.len() && (b[e].is_ascii_digit() || b[e] == b'-' || b[e] == b'+' || b[e] == b'.') {
                    e += 1;
                }
                let v = &blk[start..e];
                match c {
                    'G' => gs.push(v.parse().unwrap_or(-1)),
                    'X' => x = Some(self.coord(v)),
                    'Y' => y = Some(self.coord(v)),
                    'I' => i = Some(self.coord(v)),
                    'J' => j = Some(self.coord(v)),
                    'D' => d = v.parse().ok(),
                    _ => {}
                }
                k = e.max(k + 1);
            } else {
                k += 1;
            }
        }

        for g in gs {
            match g {
                1 => self.interp = Interp::Linear,
                2 => self.interp = Interp::Cw,
                3 => self.interp = Interp::Ccw,
                36 => {
                    self.flush_stroke();
                    self.in_region = true;
                    self.contour.clear();
                }
                37 => self.end_region(),
                74 => self.multi_quadrant = false,
                75 => self.multi_quadrant = true,
                70 => self.scale = 25.4,
                71 => self.scale = 1.0,
                90 | 54 | 55 => {}
                91 => self.warn("Incremental coordinates (G91) are not supported"),
                _ => {}
            }
        }

        // aperture selection
        if let Some(dc) = d {
            if dc >= 10 {
                self.flush_stroke();
                if !self.apertures.contains_key(&dc) {
                    self.warn(format!("Aperture D{dc} is used but never defined"));
                }
                self.cur_ap = Some(dc);
                if x.is_none() && y.is_none() {
                    return;
                }
            }
        }

        let has_coords = x.is_some() || y.is_some() || i.is_some() || j.is_some();
        let op = match d {
            Some(op @ 1..=3) => op,
            _ if has_coords => self.last_op, // modal operation code (legacy)
            _ => return,
        };
        self.last_op = op;

        let target = [x.unwrap_or(self.pos[0]), y.unwrap_or(self.pos[1])];
        match op {
            1 => self.draw(target, [i.unwrap_or(0.0), j.unwrap_or(0.0)]),
            2 => self.move_to(target),
            3 => self.do_flash(target),
            _ => {}
        }
    }

    fn draw(&mut self, target: P, ij: P) {
        let pts = if self.interp == Interp::Linear {
            vec![target]
        } else {
            arc_points(self.pos, target, ij, self.interp == Interp::Cw, self.multi_quadrant)
        };
        if self.in_region {
            if self.contour.is_empty() {
                self.contour.push(self.pos);
            }
            self.contour.extend(pts);
        } else {
            if self.stroke.is_empty() {
                self.stroke.push(self.pos);
            }
            self.stroke.extend(pts);
        }
        self.pos = target;
    }

    fn move_to(&mut self, target: P) {
        if self.in_region {
            self.finish_contour();
            self.contour.push(target);
        } else {
            self.flush_stroke();
        }
        self.pos = target;
    }

    fn do_flash(&mut self, target: P) {
        self.flush_stroke();
        if !self.in_region {
            if let Some(ap) = self.cur_ap.and_then(|c| self.apertures.get(&c)).cloned() {
                let items = flash(&ap, target, self.dark, &self.macros);
                self.items.extend(items);
                self.out.n_flashes += 1;
            }
        }
        self.pos = target;
    }

    fn flush_stroke(&mut self) {
        if self.stroke.len() >= 2 {
            let width = self
                .cur_ap
                .and_then(|c| self.apertures.get(&c))
                .map(|a| a.stroke_width())
                .unwrap_or(0.0);
            if matches!(
                self.cur_ap.and_then(|c| self.apertures.get(&c)),
                Some(Aperture::Rect { .. }) | Some(Aperture::Macro { .. })
            ) {
                self.warn("Non-circular apertures used for drawing are approximated by round strokes");
            }
            let pts = std::mem::take(&mut self.stroke);
            self.items.push(Item { prim: Prim::Stroke { pts, width }, dark: self.dark });
            self.out.n_draws += 1;
        } else {
            self.stroke.clear();
        }
    }

    fn finish_contour(&mut self) {
        if self.contour.len() >= 3 {
            let pts = std::mem::take(&mut self.contour);
            self.items.push(Item { prim: Prim::Poly { pts }, dark: self.dark });
            self.out.n_regions += 1;
        } else {
            self.contour.clear();
        }
    }

    fn end_region(&mut self) {
        self.finish_contour();
        self.in_region = false;
    }

    fn finish(mut self) -> GerberLayer {
        self.flush_stroke();
        if self.in_region {
            self.end_region();
        }
        self.close_sr();

        let mut mb = MeshBuilder::new();
        for it in &self.items {
            let clear = !it.dark;
            match &it.prim {
                Prim::Disc { c, r } => mb.disc(*c, *r, clear),
                Prim::Poly { pts } => mb.poly(pts, clear),
                Prim::Stroke { pts, width } => mb.stroke(pts, *width, clear),
            }
        }
        let mut out = self.out;
        if !mb.verts.is_empty() {
            out.bounds = Some((mb.min, mb.max));
        }
        out.verts = mb.verts;
        out.indices = mb.indices;
        out
    }
}

/// Points along an arc (excluding `start`, including `end`).
fn arc_points(start: P, end: P, ij: P, cw: bool, multi: bool) -> Vec<P> {
    let same = (start[0] - end[0]).abs() < 1e-9 && (start[1] - end[1]).abs() < 1e-9;

    let sweep_for = |c: P| -> f64 {
        let a0 = (start[1] - c[1]).atan2(start[0] - c[0]);
        let a1 = (end[1] - c[1]).atan2(end[0] - c[0]);
        let mut s = a1 - a0;
        if cw {
            while s > 1e-12 {
                s -= TAU;
            }
            if same && multi {
                s = -TAU;
            }
        } else {
            while s < -1e-12 {
                s += TAU;
            }
            if same && multi {
                s = TAU;
            }
        }
        s
    };

    let centre = if multi {
        [start[0] + ij[0], start[1] + ij[1]]
    } else {
        // Single-quadrant: I/J are unsigned. Pick the sign combination that gives
        // equal radii and a sweep of at most 90 degrees.
        let (ai, aj) = (ij[0].abs(), ij[1].abs());
        let mut best = [start[0] + ai, start[1] + aj];
        let mut best_err = f64::INFINITY;
        for (si, sj) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let c = [start[0] + si * ai, start[1] + sj * aj];
            let r0 = ((start[0] - c[0]).powi(2) + (start[1] - c[1]).powi(2)).sqrt();
            let r1 = ((end[0] - c[0]).powi(2) + (end[1] - c[1]).powi(2)).sqrt();
            let mut err = (r0 - r1).abs();
            if sweep_for(c).abs() > PI / 2.0 + 1e-6 {
                err += 1e3;
            }
            if err < best_err {
                best_err = err;
                best = c;
            }
        }
        best
    };

    let sweep = sweep_for(centre);
    let a0 = (start[1] - centre[1]).atan2(start[0] - centre[0]);
    let r0 = ((start[0] - centre[0]).powi(2) + (start[1] - centre[1]).powi(2)).sqrt();
    let r1 = ((end[0] - centre[0]).powi(2) + (end[1] - centre[1]).powi(2)).sqrt();
    let r = r0.max(r1);

    let step = if r > TOLERANCE_MM { 2.0 * (1.0 - TOLERANCE_MM / r).acos() } else { 0.5 };
    let n = ((sweep.abs() / step.max(0.01)).ceil() as usize).clamp(1, 720);

    let mut pts = Vec::with_capacity(n);
    for k in 1..n {
        let t = k as f64 / n as f64;
        let a = a0 + sweep * t;
        let rr = r0 + (r1 - r0) * t;
        pts.push([centre[0] + rr * a.cos(), centre[1] + rr * a.sin()]);
    }
    pts.push(end);
    pts
}

// -------------------------------------------------------------- public API ----

/// Parse Gerber (RS-274X) text into a renderable mesh.
pub fn parse(src: &str) -> GerberLayer {
    let mut p = Parser::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let end = src[i + 1..].find('%').map(|e| i + 1 + e).unwrap_or(src.len());
                p.extended(&src[i + 1..end]);
                i = end + 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let end = src[i..].find('*').map(|e| i + e).unwrap_or(src.len());
                let blk = src[i..end].to_string();
                if blk.trim_start().starts_with("M02") {
                    break;
                }
                p.block(&blk);
                i = end + 1;
            }
        }
    }
    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_pad_and_track() {
        let g = parse(
            "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.5*%\n%ADD11R,1.0X2.0*%\nD10*\nX0Y0D02*\nX100000Y0D01*\nD11*\nX0Y0D03*\nM02*\n",
        );
        let (min, max) = g.bounds.unwrap();
        assert!((max[0] - 10.25).abs() < 1e-3, "{max:?}");
        assert!((min[0] + 0.5).abs() < 1e-3, "{min:?}");
        assert_eq!(g.n_flashes, 1);
        assert_eq!(g.n_draws, 1);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    #[test]
    fn concave_region_triangulates_to_full_area() {
        // L-shape, area = 3
        let pts = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        let tris = triangulate(&pts);
        let area: f64 = tris.iter().map(|t| cross3(pts[t[0]], pts[t[1]], pts[t[2]]).abs() / 2.0).sum();
        assert!((area - 3.0).abs() < 1e-9, "area {area}");
    }

    #[test]
    fn macro_roundrect_with_variables() {
        let g = parse(
            "%FSLAX46Y46*%\n%MOMM*%\n%AMRR*\n0 test*\n$4=$1-$2*\n21,1,$3,$4,0,0,0*%\n%ADD10RR,0.1X0.2X1.0*%\nD10*\nX0Y0D03*\nM02*\n",
        );
        assert!(g.bounds.is_some());
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    #[test]
    fn arc_full_circle() {
        let g = parse(
            "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nG75*\nX100000Y0D02*\nG03*\nX100000Y0I-100000J0D01*\nM02*\n",
        );
        let (min, max) = g.bounds.unwrap();
        assert!((max[0] - 10.05).abs() < 0.01 && (min[0] + 10.05).abs() < 0.01, "{min:?} {max:?}");
        assert!((max[1] - 10.05).abs() < 0.01, "{max:?}");
    }

    #[test]
    fn inches_and_step_repeat() {
        let g = parse(
            "%FSLAX24Y24*%\n%MOIN*%\n%ADD10C,0.01*%\nD10*\n%SRX2Y1I1.0J0*%\nX0Y0D03*\n%SR*%\nM02*\n",
        );
        let (_, max) = g.bounds.unwrap();
        assert!((max[0] - (25.4 + 0.127)).abs() < 1e-2, "{max:?}");
        assert_eq!(g.n_flashes, 1);
    }
}

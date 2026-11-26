use std::io::{BufReader, Cursor};
use std::path::Path;

use gerber_viewer::GerberLayer;
use gerber_viewer::gerber_types::{Command, ExtendedCode, Unit};

use crate::board::layer::{LoadedLayer, style_for};
use crate::board::zip_reader::read_zip;

const ZIP_PATTERNS: &[&str] = &[
    ".gbr", ".gtl", ".gbl", ".gts", ".gbs", ".gto", ".gbo", ".gm1", ".gko", ".gml",
];

#[derive(Default)]
pub struct LoadReport {
    pub layers: Vec<LoadedLayer>,
    pub messages: Vec<String>,
}

/// Load a single Gerber file, recording user-facing status in the report.
pub fn load_single(name: &str, bytes: Vec<u8>) -> LoadReport {
    let mut report = LoadReport::default();
    match parse_layer(name, bytes) {
        Ok(layer) => {
            let repaired = if layer.repaired {
                " (line breaks repaired)"
            } else {
                ""
            };
            report.messages.push(format!("{name}: loaded{repaired}"));
            report.layers.push(layer);
        }
        Err(error) => report.messages.push(format!("{name}: {error}")),
    }
    report
}

/// Load a Gerber file or all Gerber layers in a ZIP archive.
pub fn load_path(path: &Path) -> Result<LoadReport, String> {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return load_zip(path);
    }

    let bytes =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok(load_single(&name, bytes))
}

/// Load ZIP contents in stable name order without retaining all extracted files in memory.
pub fn load_zip(path: &Path) -> Result<LoadReport, String> {
    let Some(path_str) = path.to_str() else {
        return Err("Zip path is not valid UTF-8".into());
    };

    let mut report = LoadReport::default();
    read_zip(path_str, ZIP_PATTERNS, |entry_name, bytes| {
        let name = Path::new(&entry_name)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(entry_name);
        if name.to_ascii_lowercase().ends_with(".gbrjob") {
            return;
        }

        match parse_layer(&name, bytes) {
            Ok(layer) => {
                if layer.repaired {
                    report
                        .messages
                        .push(format!("{name}: loaded after repairing line breaks"));
                }
                report.layers.push(layer);
            }
            Err(error) => report.messages.push(format!("{name}: skipped ({error})")),
        }
    })
    .map_err(|error| format!("Cannot read zip: {error}"))?;

    if report.layers.is_empty() {
        report.messages.push("No Gerber layers found in zip".into());
    } else {
        report
            .messages
            .push(format!("Loaded {} layers from zip", report.layers.len()));
    }
    Ok(report)
}

/// Some exporters write the whole file on one line, which gerber_parser silently reads as
/// "no commands". Put every `*`-terminated command on its own line again.
fn normalize_line_breaks(bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut in_ext = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                out.push('\n');
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
            }
            '%' if !in_ext => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('%');
                in_ext = true;
            }
            '%' => {
                out.push('%');
                in_ext = false;
                if !matches!(chars.peek(), Some('\n') | Some('\r') | None) {
                    out.push('\n');
                }
            }
            '*' if !in_ext => {
                out.push('*');
                if !matches!(chars.peek(), Some('\n') | Some('\r') | Some('%') | None) {
                    out.push('\n');
                }
            }
            _ => out.push(c),
        }
    }
    out.into_bytes()
}

/// Parse one file and return its layer, units, command count, and any parser error.
fn try_parse(bytes: &[u8]) -> (GerberLayer, Unit, usize, Option<String>) {
    let mut parse_err = None;
    let doc = match gerber_viewer::gerber_parser::parse(BufReader::new(Cursor::new(bytes))) {
        Ok(doc) => doc,
        Err((doc, err)) => {
            parse_err = Some(format!("{err:?}"));
            doc
        }
    };
    let commands = doc.into_commands();
    let n_commands = commands.len();
    let unit = commands
        .iter()
        .find_map(|command| match command {
            Command::ExtendedCode(ExtendedCode::Unit(unit)) => Some(*unit),
            _ => None,
        })
        .unwrap_or(Unit::Millimeters);

    (GerberLayer::new(commands), unit, n_commands, parse_err)
}

/// Parse a Gerber file into a drawable layer.
pub fn parse_layer(name: &str, bytes: Vec<u8>) -> Result<LoadedLayer, String> {
    let n_bytes = bytes.len();
    let (mut layer, mut unit, mut n_commands, mut parse_err) = try_parse(&bytes);
    let mut repaired = false;

    if layer.try_bounding_box().is_none() && !bytes.is_empty() {
        let normalized = normalize_line_breaks(&bytes);
        let (retry_layer, retry_unit, retry_commands, retry_error) = try_parse(&normalized);
        if retry_layer.try_bounding_box().is_some() {
            (layer, unit, n_commands, parse_err) =
                (retry_layer, retry_unit, retry_commands, retry_error);
            repaired = true;
        }
    }

    if layer.try_bounding_box().is_none() {
        let why = match parse_err {
            Some(error) => format!("parse error: {error}"),
            None => "no geometry".to_string(),
        };
        return Err(format!("{why} ({n_bytes} bytes, {n_commands} commands)"));
    }

    let (color, visible) = style_for(name);
    Ok(LoadedLayer {
        name: name.to_owned(),
        layer,
        unit,
        visible,
        color,
        repaired,
    })
}

#[cfg(test)]
mod tests {
    use super::{normalize_line_breaks, parse_layer};

    const SIMPLE: &str =
        "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\nX0Y0D02*\nX100000Y0D01*\nM02*\n";

    #[test]
    fn normal_file_loads() {
        assert!(parse_layer("a.gbr", SIMPLE.as_bytes().to_vec()).is_ok());
    }

    #[test]
    fn single_line_file_loads_via_fallback() {
        let one_line = SIMPLE.replace('\n', "");
        assert!(parse_layer("a.gbr", one_line.into_bytes()).is_ok());
    }

    #[test]
    fn line_break_normalization_collapses_crlf() {
        let normalized = normalize_line_breaks(b"%MOMM*%\r\nD10*\r\n");
        assert_eq!(normalized, b"%MOMM*%\nD10*\n");
    }

    #[test]
    fn empty_buffer_reports_zero_bytes() {
        let error = parse_layer("a.gbr", Vec::new()).err().unwrap();
        assert!(error.contains("0 bytes"), "{error}");
    }
}

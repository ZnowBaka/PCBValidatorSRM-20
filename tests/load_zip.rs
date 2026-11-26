use std::fs::{self, File};
use std::io::Write;

use pcb_validator::board::loader::load_zip;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const SAMPLE_GERBER: &str = include_str!("../samples/minimal.gbr");

#[test]
fn zip_loads_gerber_layers_without_a_gui() {
    let path =
        std::env::temp_dir().join(format!("pcbvalidator-load-zip-{}.zip", std::process::id()));
    let mut archive = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    archive.start_file("board/top_copper.gbr", options).unwrap();
    archive.write_all(SAMPLE_GERBER.as_bytes()).unwrap();
    archive.start_file("board/output.gbrjob", options).unwrap();
    archive.write_all(b"{}").unwrap();
    archive.finish().unwrap();

    let result = load_zip(&path);
    fs::remove_file(&path).unwrap();
    let report = result.expect("ZIP should load");

    assert_eq!(report.layers.len(), 1);
    assert_eq!(report.layers[0].name, "top_copper.gbr");
    assert!(report.layers[0].layer.try_bounding_box().is_some());
    assert!(report.layers[0].visible);
}

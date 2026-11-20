use anyhow::anyhow;
use std::io::{BufReader};

mod zip_reader;
use crate::zip_reader::read_zip;

fn main() -> anyhow::Result<()> {
    let file_names: Vec<&str> = vec![
        "copper_top.gbr",
        "soldermask_top.gbr",
        "silkscreen_top.gbr",
        "profile.gbr",
    ];

    let _some_input = String::from("PCBForTesting_v1_Manufacturing_Output.zip");
    let gerber_files = read_zip(&_some_input, &file_names)?;

    for (name, bytes) in &gerber_files {
        let reader = BufReader::new(bytes.as_slice());
        let doc = gerber_parser::parse(reader)
            .map_err(|(_partial_doc, e)| anyhow!("failed to parse: {name} : {:?}", e))?;
        for cmd in doc.commands() {
            println!("Commands: {:?}", cmd);
        }
        for err in doc.errors() {
            println!("error: {:?}", err);
        }
    }
    Ok(())
}

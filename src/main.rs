use anyhow::anyhow;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::vec;
use zip::ZipArchive;
fn main() -> anyhow::Result<()> {
    let file_names: Vec<&str> = vec![
        "copper_top.gbr",
        "soldermask_top.gbr",
        "silkscreen_top.gbr",
        "profile.gbr",
    ];

    let mut archive = ZipArchive::new(File::open("PCBForTesing_v1_Manufacturing_Output.zip")?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();
        println!("entry name: {}", name);
        for file_name in &file_names {
            if name.contains(&file_name.to_string()) {
                let mut file_to_read = Vec::new();
                entry.read_to_end(&mut file_to_read)?;
                let reader = BufReader::new(Cursor::new(file_to_read));

                let doc = gerber_parser::parse(reader)
                    .map_err(|(_partial_doc, e)| anyhow!("parse failed: {:?}", e))?;

                println!("Parsed document: {:?}", doc);
                for cmd in doc.commands() {
                    println!("Command: {:?}", cmd);
                }

                print!("errors");
                for err in doc.errors() {
                    println!("Error: {:?}", err);
                }
            }
        }
    }

    Ok(())
}

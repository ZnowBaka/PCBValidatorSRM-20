use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

pub fn read_zip(zip_path: &str, file_names: &[&str]) -> Result<HashMap<String, Vec<u8>>> {
    let mut archive = ZipArchive::new(File::open(zip_path)?)?;
    let mut file_map = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();

        for names in file_names {
            if name.contains(&names.to_string()) {
                let mut buffer = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buffer)?;
                file_map.insert(name.to_string(), buffer);
            }
        }
    }
    Ok(file_map)
}

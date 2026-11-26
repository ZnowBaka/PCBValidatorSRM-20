use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

/// Visit matching files in name order, keeping only one extracted file in memory at a time.
pub(super) fn read_zip(
    zip_path: &str,
    file_names: &[&str],
    mut process_file: impl FnMut(String, Vec<u8>),
) -> Result<()> {
    let mut archive = ZipArchive::new(File::open(zip_path)?)?;
    let mut matching_entries = HashMap::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name()?;
        if file_names.iter().any(|pattern| name.contains(pattern)) {
            matching_entries.insert(name.into_owned(), i);
        }
    }

    let mut matching_entries: Vec<_> = matching_entries.into_iter().collect();
    matching_entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, index) in matching_entries {
        let mut entry = archive.by_index(index)?;
        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buffer)?;
        drop(entry);
        process_file(name, buffer);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::read_zip;

    #[test]
    fn matching_files_are_processed_in_name_order() {
        let path = std::env::temp_dir().join(format!(
            "pcbvalidator-zip-reader-{}.zip",
            std::process::id()
        ));
        let mut writer = ZipWriter::new(File::create(&path).unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for (name, contents) in [("z.gbr", b"z".as_slice()), ("a.gbr", b"a".as_slice())] {
            writer.start_file(name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();

        let mut files = Vec::new();
        let result = read_zip(path.to_str().unwrap(), &[".gbr"], |name, bytes| {
            files.push((name, bytes));
        });
        fs::remove_file(&path).unwrap();

        assert!(result.is_ok());
        assert_eq!(
            files,
            vec![
                ("a.gbr".to_owned(), b"a".to_vec()),
                ("z.gbr".to_owned(), b"z".to_vec())
            ]
        );
    }
}

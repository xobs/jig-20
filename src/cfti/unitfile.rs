extern crate systemd_parser;
use std::io::Read;
use std::fs::File;

#[derive(Debug)]
pub enum UnitFileError {
    FileUnreadable(String),
    FileReadError(String),
    FileParseError(String),
}

pub struct UnitFile {
    unitfile: systemd_parser::items::SystemdUnit,
}

impl UnitFile {
    pub fn new(path: &str) -> Result<UnitFile, UnitFileError> {

        let mut contents = String::with_capacity(8192);
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) => return Err(UnitFileError::FileUnreadable(format!("{:?}", e))),
        };

        if let Err(e) = file.read_to_string(&mut contents) {
            return Err(UnitFileError::FileReadError(format!("{:?}", e)));
        }

        let unit_file = match systemd_parser::parse_string(&contents) {
            Ok(u) => u,
            Err(e) => return Err(UnitFileError::FileParseError(format!("{:?}", e))),
        };

        Ok(UnitFile { unitfile: unit_file })
    }

    pub fn has_section(&self, name: &str) -> bool {
        self.unitfile.has_category(name)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let coll = match self.unitfile.lookup_by_key(key) {
            // Item not found at all.
            None => return None,
            Some(s) => s,
        };

        // If it's one or many, return the first item.
        let ref coll = match coll {
            &systemd_parser::items::DirectiveEntry::Solo(ref u) => u,
            &systemd_parser::items::DirectiveEntry::Many(ref m) => &m[0],
        };

        // Item found in wrong section.
        if coll.category() != section {
            return None;
        }

        coll.value()
    }
}
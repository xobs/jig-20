extern crate ini;

use self::ini::Ini;

pub enum UnitFileError {
    FileNotFound,
    FileLoadError,
}

pub struct UnitFile {
    ini_file: ini::Ini,
}

impl UnitFile {
    pub fn new(path: &str) -> Result<UnitFile, UnitFileError> {
        let ini_file = match Ini::load_from_file(path) {
            Err(_) => return Err(UnitFileError::FileLoadError),
            Ok(s) => s,
        };

        Ok(UnitFile {
            ini_file: ini_file,
        })
    }

    pub fn has_section(&self, name: &str) -> bool {
        return self.ini_file.section(Some(name)).is_some()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&String> {
        match self.ini_file.section(Some(section)) {
            None => None,
            Some(sec) => sec.get(key),
        }
    }
}
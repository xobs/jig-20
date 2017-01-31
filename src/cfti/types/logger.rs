extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use cfti::types::Jig;

#[derive(Debug)]
enum LoggerFormat {
    TabSeparatedValue,
    JSON,
}

#[derive(Debug)]
pub enum LoggerError {
    FileLoadError,
    MissingLoggerSection,
    InvalidType(String),
}

#[derive(Debug)]
pub struct Logger {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this logger.
    name: String,

    /// description: Paragraph describing this logger.
    description: Option<String>,

    /// jigs: A collection of jig objects that this logger is compatibie with.
    //jigs: Vec<Jig>

    /// format: The format requested by this logger.
    format: LoggerFormat,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,
}

impl Logger {
    pub fn new(id: &str, path: &str, jigs: &HashMap<String, Jig>) -> Option<Result<Logger, LoggerError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(LoggerError::FileLoadError)),
            Ok(s) => s,
        };

        let logger_section = match ini_file.section(Some("Logger")) {
            None => return Some(Err(LoggerError::MissingLoggerSection)),
            Some(s) => s,
        };

        // Check to see if this logger is compatible with this jig.
        match logger_section.get("Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> = s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break
                    }
                }
                if found_it == false {
                    println!("The logger '{}' is not compatible with this jig", id);
                    return None;
                }
            }
        }

        let description = match logger_section.get("Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match logger_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match logger_section.get("ExecStart") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let format = match logger_section.get("Format") {
            None => LoggerFormat::TabSeparatedValue,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "tsv" => LoggerFormat::TabSeparatedValue,
                "json" => LoggerFormat::JSON,
                _ => return Some(Err(LoggerError::InvalidType(s.clone()))),
            },
        };

       Some(Ok(Logger {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            format: format,
       }))
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
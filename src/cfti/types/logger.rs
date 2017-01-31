extern crate ini;
use self::ini::Ini;

#[derive(Debug)]
enum LoggerFormat {
    TabSeparatedValue,
    JSON,
}

#[derive(Debug)]
pub struct Logger {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this logger.
    name: String,

    /// description: Paragraph describing this logger.
    description: Option<String>,

    /// jig_names: A list of jigs that this logger is compatibie with.
    jig_names: Option<Vec<String>>,

    /// jigs: A collection of jig objects that this logger is compatibie with.
    //jigs: Vec<Jig>

    /// format: The format requested by this logger.
    format: LoggerFormat,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,
}

impl Logger {
    pub fn new(id: &str, path: &str) -> Result<Logger, &'static str> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err("Unable to load logger file"),
            Ok(s) => s,
        };

        let logger_section = match ini_file.section(Some("Logger")) {
            None => return Err("Configuration is missing '[Logger]' section"),
            Some(s) => s,
        };

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
                _ => return Err("Test has invalid 'Type'")
            },
        };

        let jig_names = match logger_section.get("Jigs") {
            None => None,
            Some(s) => Some(s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect()),
        };

       Ok(Logger {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            jig_names: jig_names,
            format: format,
        })
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
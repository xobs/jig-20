extern crate ini;
use self::ini::Ini;

#[derive(Debug)]
pub struct Trigger {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this trigger.
    name: String,

    /// description: Paragraph describing this trigger.
    description: Option<String>,

    /// jig_names: A list of jigs that this trigger is compatibie with.
    jig_names: Vec<String>,

    /// jigs: A collection of jig objects that this trigger is compatibie with.
    //jigs: Vec<Jig>

    /// exec_start: A command to run to monitor for triggers.
    exec_start: String,
}

impl Trigger {
    pub fn new(id: &str, path: &str) -> Result<Trigger, &'static str> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err("Unable to load trigger file"),
            Ok(s) => s,
        };

        let trigger_section = match ini_file.section(Some("Trigger")) {
            None => return Err("Configuration is missing '[Trigger]' section"),
            Some(s) => s,
        };

        let description = match trigger_section.get("Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match trigger_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match trigger_section.get("ExecStart") {
            None => return Err("Trigger is missing ExecStart"),
            Some(s) => s.to_string(),
        };

        let jig_names = match trigger_section.get("Jigs") {
            None => Vec::new(),
            Some(s) => s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect(),
        };

       Ok(Trigger {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            jig_names: jig_names,
        })
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
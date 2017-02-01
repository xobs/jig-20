extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use cfti::types::Jig;
use super::super::testset::TestSet;
use super::super::process;
use std::process::{Command, Stdio};
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
enum InterfaceFormat {
    TabSeparatedValue,
    JSON,
}

#[derive(Debug)]
pub enum InterfaceError {
    FileLoadError,
    MissingInterfaceSection,
    MissingExecSection,
    MakeCommandFailed,
    ExecCommandFailed,
    InvalidType(String),
}

#[derive(Debug)]
pub struct Interface {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this logger.
    name: String,

    /// description: Paragraph describing this logger.
    description: Option<String>,

    /// format: The format requested by this interface.
    format: InterfaceFormat,

    /// exec_start: A command to run when starting the interface.
    exec_start: String,
}

impl Interface {
    pub fn new(ts: &TestSet, id: &str, path: &str, jigs: &HashMap<String, Jig>) -> Option<Result<Interface, InterfaceError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(InterfaceError::FileLoadError)),
            Ok(s) => s,
        };

        let interface_section = match ini_file.section(Some("Interface")) {
            None => return Some(Err(InterfaceError::MissingInterfaceSection)),
            Some(s) => s,
        };

        // Check to see if this logger is compatible with this jig.
        match interface_section.get("Jigs") {
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
                    ts.debug("interface", id, format!("The interface '{}' is not compatible with this jig", id).as_str());
                    return None;
                }
            }
        }

        let description = match interface_section.get("Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match interface_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match interface_section.get("ExecStart") {
            None => return Some(Err(InterfaceError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let format = match interface_section.get("Format") {
            None => InterfaceFormat::TabSeparatedValue,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "tsv" => InterfaceFormat::TabSeparatedValue,
                "json" => InterfaceFormat::JSON,
                _ => return Some(Err(InterfaceError::InvalidType(s.clone()))),
            },
        };

       Some(Ok(Interface {
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

    pub fn start(&self, ts: &TestSet) -> Result<(), InterfaceError> {
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => { println!(">>> UNABLE TO RUN INTERFACE: {:?}", e); ts.debug("interface", self.id.as_str(), format!("Unable to run logger: {:?}", e).as_str()); return Err(InterfaceError::MakeCommandFailed) },
        };
        cmd.stdout(Stdio::null());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        let child = match cmd.spawn() {
            Err(e) => { println!("Unable to spawn {:?}: {}", cmd, e); return Err(InterfaceError::ExecCommandFailed) },
            Ok(s) => s,
        };
        let mut stdin = Arc::new(Mutex::new(child.stdin.unwrap()));
        ts.start_logger(move |msg| {writeln!(stdin.lock().unwrap(), "{:?}", msg);});

        Ok(())
    }
}
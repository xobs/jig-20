extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use cfti::types::Jig;
use super::super::testset::TestSet;
use super::super::controller;
use super::super::process;
use std::process::{Command, Stdio, ChildStdin};
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;
use std::io::Write;

#[derive(Debug)]
enum InterfaceFormat {
    Text,
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

    /// working_directory: The path where the program will be run from.
    working_directory: Option<String>,
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

        let working_directory = match interface_section.get("WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let format = match interface_section.get("Format") {
            None => InterfaceFormat::Text,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "text" => InterfaceFormat::Text,
                "json" => InterfaceFormat::JSON,
                _ => return Some(Err(InterfaceError::InvalidType(s.clone()))),
            },
        };

       Some(Ok(Interface {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            working_directory: working_directory,
            format: format,
       }))
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }

    fn interface_text_write(stdin: Arc<Mutex<ChildStdin>>, msg: controller::Message) {
        writeln!(stdin.lock().unwrap().deref_mut(), "{}\t{}\t{}\t{}\t{}\t{}\t",
                                                                msg.message_type,
                                                                msg.unit,
                                                                msg.unit_type,
                                                                msg.unix_time,
                                                                msg.unix_time_nsecs,
                                                                "xx".to_string());

    }
    fn interface_json_write(stdin: Arc<Mutex<ChildStdin>>, msg: controller::Message) {
    }

    pub fn start(&self, ts: &TestSet) -> Result<(), InterfaceError> {
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => { println!(">>> UNABLE TO RUN INTERFACE: {:?}", e); ts.debug("interface", self.id.as_str(), format!("Unable to run logger: {:?}", e).as_str()); return Err(InterfaceError::MakeCommandFailed) },
        };
        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        match self.working_directory {
            None => (),
            Some(ref s) => {cmd.current_dir(s); },
        }

        let child = match cmd.spawn() {
            Err(e) => { println!("Unable to spawn {:?}: {}", cmd, e); return Err(InterfaceError::ExecCommandFailed) },
            Ok(s) => s,
        };
        let mut stdin = Arc::new(Mutex::new(child.stdin.unwrap()));
        let mut stdout = Arc::new(Mutex::new(child.stdout.unwrap()));

        match self.format {
            InterfaceFormat::Text => ts.monitor_broadcasts(move |msg| {Interface::interface_text_write(stdin.clone(), msg);}),
            InterfaceFormat::JSON => ts.monitor_broadcasts(move |msg| {Interface::interface_json_write(stdin.clone(), msg);}),
        };
        Ok(())
    }
}
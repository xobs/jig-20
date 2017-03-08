extern crate json;

use cfti::types::Jig;
use cfti::types::unit::Unit;
use cfti::controller::{Controller, ControlMessageContents, BroadcastMessage, BroadcastMessageContents};
use cfti::process;
use cfti::unitfile::UnitFile;

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::fmt::{Formatter, Display, Error};

#[derive(Debug, Clone)]
enum LoggerFormat {
    TabSeparatedValue,
    JSON,
}

#[derive(Debug)]
pub enum LoggerError {
    FileLoadError(String),
    MissingLoggerSection,
    MissingExecSection,
    ExecCommandFailed,
    InvalidType(String),
}

impl Display for LoggerError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            &LoggerError::FileLoadError(ref s) => write!(f, "Unable to load file: {}", s),
            &LoggerError::MissingLoggerSection => write!(f, "Unit file is missing logger section"),
            &LoggerError::MissingExecSection => write!(f, "Unit file is missing exec section"),
            &LoggerError::ExecCommandFailed => write!(f, "Unable to exec command"),
            &LoggerError::InvalidType(ref s) => write!(f, "Invalid logger type: {}", s),
        }
    }
}

#[derive(Debug)]
pub struct Logger {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this logger.
    name: String,

    /// description: Paragraph describing this logger.
    description: Option<String>,

    /// format: The format requested by this logger.
    format: LoggerFormat,

    /// exec_start: A command to run when starting tests.
    exec_start: String,

    /// working_directory: The path where exec_start will be run from.
    working_directory: Option<String>,

    /// The master controller, where bus messages come and go.
    controller: Controller,
}

impl Logger {
    pub fn new(id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               controller: &Controller) -> Option<Result<Logger, LoggerError>> {

        // Load the .ini file
        let unitfile = match UnitFile::new(path) {
            Err(e) => return Some(Err(LoggerError::FileLoadError(format!("{:?}", e)))),
            Ok(s) => s,
        };

        if ! unitfile.has_section("Logger") {
            return Some(Err(LoggerError::MissingLoggerSection));
        }

        // Check to see if this logger is compatible with this jig.
        match unitfile.get("Logger", "Jigs") {
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
                    controller.control_class(
                                  "debug",
                                  id,
                                  "logger",
                                  &ControlMessageContents::Log(format!("The logger '{}' is not compatible with this jig", id)));
                    return None;
                }
            }
        }

        let description = match unitfile.get("Logger", "Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match unitfile.get("Logger", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let working_directory = match unitfile.get("Logger", "WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let exec_start = match unitfile.get("Logger", "ExecStart") {
            None => return Some(Err(LoggerError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let format = match unitfile.get("Logger", "Format") {
            None => LoggerFormat::TabSeparatedValue,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "tsv" => LoggerFormat::TabSeparatedValue,
                "json" => LoggerFormat::JSON,
                _ => return Some(Err(LoggerError::InvalidType(s.to_string()))),
            },
        };

       Some(Ok(Logger {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            working_directory: working_directory,
            format: format,
            controller: controller.clone(),
       }))
    }

    pub fn start(&self, working_directory: &Option<String>) -> Result<(), LoggerError> {

        let working_directory = match *working_directory {
            Some(ref s) => Some(s.clone()),
            None => match self.working_directory {
                Some(ref s) => Some(s.clone()),
                None => None,
            },
        };

        self.debug(format!("Starting logger..."));
        let process = match process::spawn_cmd(self.exec_start.as_str(),
                                               self,
                                               &working_directory,
                                               &self.controller) {
            Err(e) => {
                self.debug(format!("Unable to spawn {}: {:?}", self.exec_start, e));
                return Err(LoggerError::ExecCommandFailed);
            },
            Ok(s) => s,
        };

        let mut stdin = process.stdin;
        let unit = self.to_simple_unit();

        match self.format {
            LoggerFormat::TabSeparatedValue => self.controller.listen_logs(move |msg| {
                match msg {
                    BroadcastMessage { message: BroadcastMessageContents::Log(log), .. } => 
                        if let Err(e) = writeln!(&mut stdin, "{}\t{}\t{}\t{}\t{}\t{}\t",
                                        msg.message_class,
                                        msg.unit_id,
                                        msg.unit_type,
                                        msg.unix_time,
                                        msg.unix_time_nsecs,
                                        log.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")) {
                            Controller::debug_unit(&unit, format!("Unable to write to logfile: {:?}", e));
                            return Err(());
                        },
                    _ => (),
                };
                Ok(())
            }),
            LoggerFormat::JSON => self.controller.listen_logs(move |msg| {
                match msg {
                    BroadcastMessage { message: BroadcastMessageContents::Log(log), .. } => {
                        let mut object = json::JsonValue::new_object();
                        object["message_class"] = msg.message_class.into();
                        object["unit_id"] = msg.unit_id.into();
                        object["unit_type"] = msg.unit_type.into();
                        object["unix_time"] = msg.unix_time.into();
                        object["unix_time_nsecs"] = msg.unix_time_nsecs.into();
                        object["message"] = log.into();
                        if let Err(e) = writeln!(&mut stdin, "{}", json::stringify(object)) {
                            Controller::debug_unit(&unit, format!("Unable to write to logfile: {:?}", e));
                            return Err(());
                        };
                    },
                    _ => (),
                }
                Ok(())
            }),
        };

        self.debug(format!("Logger is running"));
        Ok(())
    }
}

impl Unit for Logger {
    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn kind(&self) -> &str {
        "logger"
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn description(&self) -> &str {
        match self.description {
            Some(ref s) => s.as_str(),
            None => "",
        }
    }

    fn controller(&self) -> &Controller {
        &self.controller
    }
}
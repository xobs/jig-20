extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use cfti::types::Jig;
use super::super::testset::TestSet;
use super::super::controller::{Message, MessageContents};
use super::super::process;
use std::process::Stdio;
use std::io::Write;
use std::sync::{Arc, Mutex};
extern crate json;

#[derive(Debug, Clone)]
enum LoggerFormat {
    TabSeparatedValue,
    JSON,
}

#[derive(Debug)]
pub enum LoggerError {
    FileLoadError,
    MissingLoggerSection,
    MissingExecSection,
    MakeCommandFailed,
    ExecCommandFailed,
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

    /// format: The format requested by this logger.
    format: LoggerFormat,

    /// exec_start: A command to run when starting tests.
    exec_start: String,

    /// working_directory: The path where exec_start will be run from.
    working_directory: Option<String>,
}

impl Logger {
    pub fn new(ts: &TestSet, id: &str, path: &str, jigs: &HashMap<String, Jig>) -> Option<Result<Logger, LoggerError>> {

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
                    ts.debug("logger", id, format!("The logger '{}' is not compatible with this jig", id).as_str());
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

        let working_directory = match logger_section.get("WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let exec_start = match logger_section.get("ExecStart") {
            None => return Some(Err(LoggerError::MissingExecSection)),
            Some(s) => s.to_string(),
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
            working_directory: working_directory,
            format: format,
       }))
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }

    pub fn start(&self, ts: &TestSet) -> Result<(), LoggerError> {
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => { println!(">>> UNABLE TO RUN LOGGER: {:?}", e); ts.debug("logger", self.id.as_str(), format!("Unable to run logger: {:?}", e).as_str()); return Err(LoggerError::MakeCommandFailed) },
        };
        cmd.stdout(Stdio::null());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        match self.working_directory {
            None => (),
            Some(ref s) => {cmd.current_dir(s); },
        }

        let child = match cmd.spawn() {
            Err(e) => { println!("Unable to spawn {:?}: {}", cmd, e); return Err(LoggerError::ExecCommandFailed) },
            Ok(s) => s,
        };
        let mut stdin = Arc::new(Mutex::new(child.stdin.unwrap()));
        let format = self.format.clone();
        match format {
            LoggerFormat::TabSeparatedValue => ts.monitor_logs(move |msg| {
                match msg {
                    Message { message: MessageContents::Log(log), .. } => 
                        writeln!(stdin.lock().unwrap(), "{}\t{}\t{}\t{}\t{}\t{}\t",
                                        msg.message_type,
                                        msg.unit,
                                        msg.unit_type,
                                        msg.unix_time,
                                        msg.unix_time_nsecs,
                                        log.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")),
                    _ => return,
                };
            }),
            LoggerFormat::JSON => ts.monitor_logs(move |msg| {
                match msg {
                    Message { message: MessageContents::Log(log), .. } => {
                        let mut object = json::JsonValue::new_object();
                        object["message_type"] = msg.message_type.into();
                        object["unit"] = msg.unit.into();
                        object["unit_type"] = msg.unit_type.into();
                        object["unix_time"] = msg.unix_time.into();
                        object["unix_time_nsecs"] = msg.unix_time_nsecs.into();
                        object["message"] = log.into();
                        writeln!(stdin.lock().unwrap(), "{}", json::stringify(object));
                    },
                    _ => return,
                }
            }),
        };

        Ok(())
    }
}
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cfti::unitfile::UnitFile;
use cfti::types::Jig;
use cfti::controller::{Controller, ControlMessageContents};
use cfti::config;
use cfti::process;

#[derive(Debug)]
pub enum TriggerError {
    FileLoadError(String),
    MissingTriggerSection,
    MissingExecStart,
    TriggerSpawnError(process::CommandError),
}

#[derive(Debug)]
pub struct Trigger {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this trigger.
    name: String,

    /// description: Paragraph describing this trigger.
    description: Option<String>,

    /// exec_start: A command to run to monitor for triggers.
    exec_start: String,

    /// Optional working directory for the trigger
    working_directory: Option<String>,

    /// The controller where messages come and go.
    controller: Controller,
}

impl Trigger {
    pub fn new(id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               config: &config::Config,
               controller: &Controller) -> Option<Result<Trigger, TriggerError>> {

        // Load the .ini file
        let unitfile = match UnitFile::new(path) {
            Err(e) => return Some(Err(TriggerError::FileLoadError(format!("{:?}", e)))),
            Ok(s) => s,
        };

        if ! unitfile.has_section("Trigger") {
            return Some(Err(TriggerError::MissingTriggerSection));
        }

        let description = match unitfile.get("Trigger", "Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match unitfile.get("Trigger", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let exec_start = match unitfile.get("Trigger", "ExecStart") {
            None => return Some(Err(TriggerError::MissingExecStart)),
            Some(s) => s.to_string(),
        };

        let working_directory = match unitfile.get("Trigger", "WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        // Check to see if this interface is compatible with this jig.
        match unitfile.get("Trigger", "Jigs") {
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
                    controller.debug("trigger", id, format!("The trigger '{}' is not compatible with this jig", id));
                    return None;
                }
            }
        }

       Some(Ok(Trigger {
            id: id.to_string(),
            name: name,
            description: description,
            exec_start: exec_start,
            working_directory: working_directory,
            controller: controller.clone(),
       }))
    }

    fn cfti_unescape(msg: String) -> String {
        msg.replace("\\t", "\t").replace("\\n", "\n").replace("\\r", "\r").replace("\\\\", "\\")
    }

    fn read_line(line: String, id: &str, controller: &Controller) -> Result<(), ()> {
        controller.debug(id, "trigger", format!("CFTI trigger input: {}", line));
        let mut words: Vec<String> = line.split_whitespace().map(|x| Self::cfti_unescape(x.to_string())).collect();

        // Don't crash if we get a blank line.
        if words.len() == 0 {
            return Ok(());
        }

        let verb = words[0].to_lowercase();
        words.remove(0);

        let response = match verb.as_str() {
            "start" => if words.len() > 0 {
                ControlMessageContents::StartScenario(Some(words[0].clone()))
            } else {
                ControlMessageContents::StartScenario(None)
            },
            "stop" => ControlMessageContents::AbortTests,
            "hello" => ControlMessageContents::Hello(words.join(" ")),
            "log" => ControlMessageContents::Log(words.join(" ")),
            _ => ControlMessageContents::Log(format!("Unimplemented verb: {}", verb)),
        };
        controller.control(id, "trigger", &response);
        Ok(())
    }

    pub fn start(&self, working_directory: &Option<String>)
             -> Result<(), TriggerError> {

        let working_directory = match *working_directory {
            Some(ref s) => Some(s.clone()),
            None => match self.working_directory {
                Some(ref s) => Some(s.clone()),
                None => None,
            },
        };

        let cmd = match process::spawn_cmd(self.exec_start.as_str(),
                                           self.id(),
                                           self.kind(),
                                           &working_directory,
                                           &self.controller) {
            Err(e) => return Err(TriggerError::TriggerSpawnError(e)),
            Ok(o) => o,
        };

        let thr_controller = self.controller.clone();
        let thr_id = self.id().to_string();
        process::log_output(cmd.stderr, &self.controller, self.id(), self.kind(), "stderr");
        process::watch_output(cmd.stdout, &self.controller, self.id(), self.kind(), move |line| {
            Self::read_line(line, &thr_id, &thr_controller)
        });
        Ok(())
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn kind(&self) -> &str {
        "trigger"
    }
}
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cfti::unitfile::UnitFile;
use cfti::types::Jig;
use cfti::controller::{self, Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::config;

#[derive(Debug)]
pub enum TriggerError {
    FileLoadError(String),
    MissingTriggerSection,
    MissingExecStart,
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
       }))
    }

    pub fn start(&self) ->  Result<(), TriggerError> {
        Ok(())
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
use std::path::Path;

use cfti::process;
use cfti::config;
use cfti::controller::{Controller, BroadcastMessageContents};
use cfti::unitfile::UnitFile;
use cfti::types::unit::Unit;

#[derive(Debug)]
pub enum JigError {
    FileLoadError(String),
    MissingJigSection,
}

#[derive(Debug)]
pub struct Jig {

    /// Id: File name on disk, what other units refer to this one as.
    id: String,

    /// Name: Defines the short name for this jig.
    name: String,

    /// Description: Defines a detailed description of this jig.  May be up to one paragraph.
    description: String,

    /// DefaultScenario: Name of the scenario to run by default.
    default_scenario: Option<String>,

    /// WorkingDirectory: The default directory for programs on this jig.
    working_directory: Option<String>,

    /// The controller where messages go.
    controller: Controller,
}

impl Jig {
    pub fn new(id: &str,
               path: &str,
               config: &config::Config,
               controller: &Controller) -> Option<Result<Jig, JigError>> {

        // Load the .ini file
        let unitfile = match UnitFile::new(path) {
            Err(e) => return Some(Err(JigError::FileLoadError(format!("{:?}", e)))),
            Ok(s) => s,
        };

        // Make sure there is a "Jig" section.
        if ! unitfile.has_section("Jig") {
            return Some(Err(JigError::MissingJigSection));
        }

        // Determine if this is the jig we're running on
        if let Some(s) = unitfile.get("Jig", "TestFile") {
            if !Path::new(s).exists() {
                controller.debug("jig", id, format!("Test file {} DOES NOT EXIST", s));
                return None;
            };
            controller.debug("jig", id, format!("Test file {} exists", s));
        };

        let working_directory = match unitfile.get("Jig", "WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        if let Some(s) = unitfile.get("Jig", "TestProgram") {
            if !process::try_command(&controller, s, &working_directory, config.timeout()) {
                controller.debug("jig", id, format!("Test program FAILED"));
                return None;
            }
        };

        let description = match unitfile.get("Jig", "Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match unitfile.get("Jig", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let default_scenario = match unitfile.get("Jig", "DefaultScenario") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let working_directory = match unitfile.get("Jig", "DefaultWorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        Some(Ok(Jig {
            id: id.to_string(),
            name: name,
            description: description,

            default_scenario: default_scenario,
            working_directory: working_directory,
            controller: controller.clone(),
        }))
    }

    pub fn describe(&self) {
        self.controller.broadcast(
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "name".to_string(),
                                                                  self.id().to_string(),
                                                                  self.name().to_string()));
        self.controller.broadcast(
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "description".to_string(),
                                                                  self.id().to_string(),
                                                                  self.description().to_string()));
    }

    pub fn default_scenario(&self) -> &Option<String> {
        &self.default_scenario
    }

    pub fn default_working_directory(&self) -> &Option<String> {
        &self.working_directory
    }
}

impl Unit for Jig {
    fn kind(&self) -> &str {
        "jig"
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn description(&self) -> &str {
        self.description.as_str()
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }
}
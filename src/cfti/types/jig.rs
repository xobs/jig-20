use std::path::Path;

use cfti::process;
use cfti::config;
use cfti::controller::{Controller, BroadcastMessageContents};
use cfti::unitfile::UnitFile;
use cfti::types::unit::Unit;
use cfti::testset;

#[derive(Debug)]
pub enum JigError {
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
               unitfile: UnitFile,
               test_set: &testset::TestSet)
               -> Option<Result<Jig, JigError>> {

        // Make sure there is a "Jig" section.
        if !unitfile.has_section("Jig") {
            return Some(Err(JigError::MissingJigSection));
        }

        // Determine if this is the jig we're running on
        if let Some(s) = unitfile.get("Jig", "TestFile") {
            if !Path::new(s).exists() {
                test_set.debug(format!("{}: Test file {} DOES NOT EXIST", id, s));
                return None;
            };
        };

        let working_directory = match unitfile.get("Unit", "WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        if let Some(s) = unitfile.get("Jig", "TestProgram") {
            if !process::try_command(test_set, s, &working_directory, test_set.config().timeout()) {
                test_set.debug(format!("{}: Test program FAILED", id));
                return None;
            }
        };

        let description = match unitfile.get("Unit", "Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match unitfile.get("Unit", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let default_scenario = match unitfile.get("Jig", "DefaultScenario") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        Some(Ok(Jig {
            id: id.to_string(),
            name: name,
            description: description,

            default_scenario: default_scenario,
            working_directory: working_directory,
            controller: test_set.controller().clone(),
        }))
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

    fn controller(&self) -> &Controller {
        &self.controller
    }
}
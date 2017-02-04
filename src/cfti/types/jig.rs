extern crate ini;
use self::ini::Ini;
use std::path::Path;
use std::process::Command;
use super::super::process;
use super::super::config;
use super::super::testset::TestSet;

#[derive(Debug)]
pub enum JigError {
    FileLoadError,
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
}

impl Jig {
    pub fn new(ts: &TestSet, id: &str, path: &str) -> Option<Result<Jig, JigError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(JigError::FileLoadError)),
            Ok(s) => s,
        };

        let jig_section = match ini_file.section(Some("Jig")) {
            None => return Some(Err(JigError::MissingJigSection)),
            Some(s) => s,
        };

        // Determine if this is the jig we're running on
        match jig_section.get("TestFile") {
            None => {
                ts.debug("jig", id, "Test file not specified, skipping");
                ()
            },
            Some(s) => {
                if !Path::new(s).exists() {
                    ts.debug("jig", id, format!("Test file {} DOES NOT EXIST", s).as_str());
                    return None;
                };
                ts.debug("jig", id, "Test file exists");
                ()
            }
        };

        let working_directory = match jig_section.get("WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        match jig_section.get("TestProgram") {
            None => {
                ts.debug("jig", id, "No TestProgram specified");
                ()
            },
            Some(s) => {
                if !process::try_command(ts, s, &working_directory, config::default_timeout()) {
                    ts.debug("jig", id, "Test program FAILED");
                    return None;
                }
                ts.debug("jig", id, "Test program passed");
                ()
            },
        };

        let description = match jig_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match jig_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let default_scenario = match jig_section.get("DefaultScenario") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        Some(Ok(Jig {
            id: id.to_string(),
            name: name,
            description: description,

            default_scenario: default_scenario,
        }))
    }

    pub fn name(&self) -> &String {
        return &self.name;
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
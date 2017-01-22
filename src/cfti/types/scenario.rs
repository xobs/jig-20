extern crate ini;
use self::ini::Ini;
use super::test::Test;

#[derive(Debug)]
pub struct Scenario {
    /// id: The string that other units refer to this file as.
    id: String,

    /// tests: A vector containing all the tests in this scenario.
    tests: Vec<Test>,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,

    /// exec_stop_success: A command to run upon successful completion of this scenario.
    exec_stop_success: Option<String>,

    /// exec_stop_failure: A command to run if this scenario fails.
    exec_stop_failure: Option<String>,
}

impl Scenario {
    pub fn new(id: &str, path: &str) -> Result<Scenario, &'static str> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err("Unable to load test file"),
            Ok(s) => s,
        };

        let scenario_section = match ini_file.section(Some("Scenario")) {
            None => return Err("Test is missing '[Scenario]' section"),
            Some(s) => s,
        };


        let exec_start = match scenario_section.get("ExecStart") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_success = match scenario_section.get("ExecStopSuccess") {
            None => match scenario_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_failure = match scenario_section.get("ExecStopFail") {
            None => match scenario_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };


        Ok(Scenario {
            id: id.to_string(),
            tests: Vec::new(),
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,
        })
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
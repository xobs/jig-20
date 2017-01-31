extern crate ini;
use self::ini::Ini;
use std::sync::Arc;
use std::collections::HashMap;
use super::test::Test;
use super::super::testset::TestSet;

pub enum ScenarioError {
    TestNotFound(String),
}

#[derive(Debug)]
pub struct Scenario {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this scenario.
    name: String,

    /// description: Paragraph describing this scenario.
    description: Option<String>,

    /// timeout: Maximum number of seconds this scenario should take.
    timeout: u32,

    /// tests: A vector containing all the tests in this scenario.  Will be resolved after all units are loaded.
    pub tests: Vec<Arc<Test>>,

    /// test_names: A vector containing the names of all the tests.
    pub test_names: Vec<String>,

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
            Err(_) => return Err("Unable to load scenario file"),
            Ok(s) => s,
        };

        let scenario_section = match ini_file.section(Some("Scenario")) {
            None => return Err("Configuration is missing '[Scenario]' section"),
            Some(s) => s,
        };

        let description = match scenario_section.get("Description") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let name = match scenario_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let timeout = match scenario_section.get("Timeout") {
            None => 2000,
            Some(s) => s.parse().unwrap(),
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

        let test_names = match scenario_section.get("Tests") {
            None => return Err("Unable to find test list"),
            Some(s) => s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect(),
        };

        Ok(Scenario {
            id: id.to_string(),
            test_names: test_names,
            tests: Vec::new(),
            timeout: timeout,
            name: name,
            description: description,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,
        })
    }

    pub fn resolve_tests(&mut self, test_set: &HashMap<String, Arc<Test>>) -> Result<(), ScenarioError> {

        println!("Resolving tests for {}", self.name);
        for test_name in self.test_names.iter() {
            let test = match test_set.get(test_name) {
                None => {
                    println!("Test {} NOT FOUND", test_name);
                    return Err(ScenarioError::TestNotFound(test_name.clone()));
                },
                Some(t) => t,
            };
            self.tests.push(test.clone());
            println!("Test {} was found", test_name);
        }
        Ok(())
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
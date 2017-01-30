use cfti::types::Test;
use cfti::types::Scenario;
use cfti::types::Logger;
use cfti::types::Trigger;
/*
use cfti::types::Jig;
use cfti::types::Coupon;
use cfti::types::Interface;
use cfti::types::Updater;
use cfti::types::Service;
*/

use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::PathBuf;

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
pub struct TestSet {
    tests: HashMap<String, Test>,
    scenarios: HashMap<String, Scenario>,
    triggers: HashMap<String, Trigger>,
    loggers: HashMap<String, Logger>,
    /*
    jigs: HashMap<String, Jig>,
    coupons: HashMap<String, Coupon>,
    updaters: HashMap<String, Updater>,
    services: HashMap<String, Service>,
    */
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(dir: &str) -> Result<TestSet, Error> {

        let mut test_set = TestSet {
            tests: HashMap::new(),
            scenarios: HashMap::new(),
            loggers: HashMap::new(),
            triggers: HashMap::new(),
        };

        // Step 1: Read each unit file from the disk
        for entry in try!(fs::read_dir(dir)) {
            let file = try!(entry);
            let path = file.path();

            if !try!(file.file_type()).is_file() {
                continue;
            }

            test_set.add_item(path);
        }

        // Step 2: Resolve unit names to unit files.
        TestSet::resolve_scenarios(&mut test_set);
        Ok(test_set)
    }

    fn resolve_scenarios(test_set: &mut TestSet) {
        let scenario_names: Vec<String> = test_set.scenarios.keys().map(|s| s.clone()).collect();
        for scenario_name in scenario_names {
            println!("Scenario: {}", scenario_name);
            let ref mut scenario = test_set.scenarios.get_mut(&scenario_name).unwrap();
            for test in scenario.test_names {
                scenario.tests.push(test_set.tests.get(&test).unwrap());
            }
        }
    }
    fn add_item(&mut self, path: PathBuf) {
        let item_name = path.file_stem().unwrap().to_str().unwrap();
        let path_str = path.to_str().unwrap();

        match path.extension() {
            None => return,
            Some(entry) => {
                if entry == "test" {
                    let new_test = Test::new(item_name, path_str).unwrap();
                    self.tests.insert(new_test.id().clone(), new_test);
                } else if entry == "scenario" {
                    let new_scenario = Scenario::new(item_name, path_str).unwrap();
                    self.scenarios.insert(new_scenario.id().clone(), new_scenario);
                } else if entry == "logger" {
                    let new_logger = Logger::new(item_name, path_str).unwrap();
                    self.loggers.insert(new_logger.id().clone(), new_logger);
                } else if entry == "trigger" {
                    let new_trigger = Trigger::new(item_name, path_str).unwrap();
                    self.triggers.insert(new_trigger.id().clone(), new_trigger);
                }
                else {
                    println!("Unrecognized file type: {:?}", path);
                    return
                }
            }
        }
    }

    pub fn all_tests(&self) -> Vec<&Test> {
        let mut sorted_keys: Vec<&String> = self.tests.keys().collect();
        sorted_keys.sort();

        let mut result: Vec<&Test> = Vec::new();
        for key in sorted_keys {
            result.push(self.tests.get(key).unwrap());
        }
        result
    }
}
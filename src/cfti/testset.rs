use cfti::types::Test;
use cfti::types::Scenario;
use cfti::types::Logger;
use cfti::types::Trigger;
use cfti::types::Jig;
/*
use cfti::types::Coupon;
use cfti::types::Interface;
use cfti::types::Updater;
use cfti::types::Service;
*/

use std::collections::HashMap;
use std::sync::Arc;
use std::fs;
use std::io::Error;
use std::path::PathBuf;
use std::ffi::OsStr;

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
pub struct TestSet {
    tests: HashMap<String, Arc<Test>>,
    scenarios: HashMap<String, Scenario>,
    triggers: HashMap<String, Trigger>,
    loggers: HashMap<String, Logger>,
    jigs: HashMap<String, Jig>,
    /*
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
            jigs: HashMap::new(),
        };

        /* TestSet ordering:
         * When a TestSet is loaded off the disk, the order of unit files is
         * prioritized so that dependencies can be checked.
         *
         * The order of files to be loaded:
         *  1) Jig
         *  2) Logger
         *  3) Interface
         *  4) Service
         *  5) Updater
         *  6) Test
         *  7) Scenario
         *  8) Trigger
         *  9) Coupon
         */
        let mut jig_paths = vec![];
        let mut logger_paths = vec![];
        let mut interface_paths = vec![];
        let mut service_paths = vec![];
        let mut updater_paths = vec![];
        let mut test_paths = vec![];
        let mut scenario_paths = vec![];
        let mut trigger_paths = vec![];
        let mut coupon_paths = vec![];

        // Step 1: Read each unit file from the disk
        let entries_rd: fs::ReadDir = try!(fs::read_dir(dir));
        let mut entries: Vec<PathBuf> = vec![];
        for entry_opt in entries_rd {
            let entry = try!(entry_opt);
            let path = entry.path();
            if !try!(entry.file_type()).is_file() {
                continue;
            }

            match path.extension().unwrap_or(OsStr::new("")).to_str().unwrap_or("") {
                "jig" => jig_paths.push(path.clone()),
                "logger" => logger_paths.push(path.clone()),
                "interface" => interface_paths.push(path.clone()),
                "service" => service_paths.push(path.clone()),
                "updater" => updater_paths.push(path.clone()),
                "test" => test_paths.push(path.clone()),
                "scenario" => scenario_paths.push(path.clone()),
                "trigger" => trigger_paths.push(path.clone()),
                "coupon" => coupon_paths.push(path.clone()),
                unknown => println!("Unrecognized path: {}", path.to_str().unwrap_or("")),
            }
        }

        test_set.load_jigs(&jig_paths);
        test_set.load_loggers(&logger_paths);
        //test_set.load_interfaces(&interface_paths);
        //test_set.load_services(&service_paths);
        //test_set.load_updaters(&updater_paths);
        //test_set.load_tests(&test_paths);
        //test_set.load_scenarios(&scenario_paths);
        //test_set.load_triggers(&trigger_paths);
        //test_set.load_coupons(&coupon_paths);

        Ok(test_set)
    }

    fn load_jigs(&mut self, jig_paths: &Vec<PathBuf>) {
        for jig_path in jig_paths {
            let item_name = jig_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = jig_path.to_str().unwrap_or("");

            let new_jig = Jig::new(item_name, path_str);

            // The jig will return "None" if it is incompatible.
            if new_jig.is_none() {
                continue;
            }

            let new_jig = new_jig.unwrap();
            if new_jig.is_err() {
                println!("Unable to load jig file: {:?}", new_jig.unwrap_err());
                continue;
            }

            let new_jig = new_jig.unwrap();
            self.jigs.insert(new_jig.id().clone(), new_jig);
        }
    }

    fn load_loggers(&mut self, logger_paths: &Vec<PathBuf>) {
        for logger_path in logger_paths {
            let item_name = logger_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = logger_path.to_str().unwrap_or("");
            let new_logger = Logger::new(item_name, path_str, &self.jigs);

            // In this case, it just means the logger is incompatible.
            if new_logger.is_none() {
                continue;
            }
            let new_logger = new_logger.unwrap();

            if new_logger.is_err() {
                println!("Unable to load logger: {:?}", new_logger.unwrap_err());
                continue;
            }
            let new_logger = new_logger.unwrap();

            self.loggers.insert(new_logger.id().clone(), new_logger);
        }
    }

    fn resolve_scenarios(&mut self) {
        for (_, ref mut scenario) in self.scenarios.iter_mut() {
            scenario.resolve_tests(&self.tests);
        }
    }
    fn add_item(&mut self, path: PathBuf) {
        let item_name = path.file_stem().unwrap().to_str().unwrap();
        let path_str = path.to_str().unwrap();

        match path.extension() {
            None => return,
            Some(entry) => {
                match entry.to_str().unwrap() {
                    "test" => {
                        let new_test = Test::new(item_name, path_str).unwrap();
                        self.tests.insert(new_test.id().clone(), Arc::new(new_test));
                    },
                    "scenario" => {
                        let new_scenario = Scenario::new(item_name, path_str).unwrap();
                        self.scenarios.insert(new_scenario.id().clone(), new_scenario);
                    },
                    "trigger" => {
                        let new_trigger = Trigger::new(item_name, path_str).unwrap();
                        self.triggers.insert(new_trigger.id().clone(), new_trigger);
                    },
                    _ => {
                        println!("Unrecognized file type: {:?}", path);
                        return
                    },
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
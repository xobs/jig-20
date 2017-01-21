use cfti::types::Test;
/*
use cfti::types::Jig;
use cfti::types::Scenario;
use cfti::types::Coupon;
use cfti::types::Trigger;
use cfti::types::Logger;
use cfti::types::Interface;
use cfti::types::Updater;
use cfti::types::Service;
*/

use std::collections::HashMap;
use std::fs;
use std::io::Error;

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
pub struct TestSet {
    tests: HashMap<String, Test>,
    /*
    jigs: HashMap<String, Jig>,
    scenarios: HashMap<String, Scenario>,
    coupons: HashMap<String, Coupon>,
    triggers: HashMap<String, Trigger>,
    loggers: HashMap<String, Logger>,
    updaters: HashMap<String, Updater>,
    services: HashMap<String, Service>,
    */
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(dir: &str) -> Result<TestSet, Error> {

        let mut tests: HashMap<String, Test> = HashMap::new();
//        let mut devs: HashMap<String, TestTarget> = HashMap::new();

        for entry in try!(fs::read_dir(dir)) {
            let file = try!(entry);
            let path = file.path();

            if !try!(file.file_type()).is_file() {
                continue;
            }

            let new_test_name: String = path.file_stem().unwrap().to_str().unwrap().to_string();

            match path.extension() {
                None => continue,
                Some(entry) => if entry == "test" {
                    let new_test = Test::new(path.to_str().unwrap().to_string()).unwrap();
                    tests.insert(new_test_name.clone(), new_test);
                }
                else {
                    println!("Unrecognized file type: {:?}", file);
                    continue
                }
            }
        }

        let test_set = TestSet {
            tests: tests,
        };

        Ok(test_set)
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
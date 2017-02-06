use super::types::Test;
use super::types::Scenario;
use super::types::Logger;
use super::types::Trigger;
use super::types::Jig;
use super::types::Interface;
/*
use cfti::types::Coupon;
use cfti::types::Updater;
use cfti::types::Service;
*/
use super::controller;

use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::ffi::OsStr;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time;
use std::ops::{DerefMut, Deref};

use super::controller::{Message, MessageContents};

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
pub struct TestSet {
    controller: Arc<Mutex<controller::Controller>>,
    tests: HashMap<String, Arc<Test>>,
    scenarios: HashMap<String, Scenario>,
    triggers: HashMap<String, Trigger>,
    loggers: HashMap<String, Logger>,
    jigs: HashMap<String, Jig>,
    interfaces: HashMap<String, Interface>,

    /// The id of the jig that we've decided to use.
    jig: String,

    /// The id of the scenario that we're using.
    scenario: String,

    //messaging: Rc<RefCell<messaging::Messaging>>,
    /*
    coupons: HashMap<String, Coupon>,
    updaters: HashMap<String, Updater>,
    services: HashMap<String, Service>,
    */
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(dir: &str, controller: Arc<Mutex<controller::Controller>>) -> Result<Arc<Mutex<TestSet>>, Error> {

        // Add a simple logger to show us debug data.
        controller.lock().unwrap().add_logger(|msg| println!("DEBUG>> {:?}", msg));
        controller.lock().unwrap().add_logger(|msg| println!("DEBUG2>> {:?}", msg));

        let mut test_set = Arc::new(Mutex::new(TestSet {
            tests: HashMap::new(),
            scenarios: HashMap::new(),
            loggers: HashMap::new(),
            triggers: HashMap::new(),
            jigs: HashMap::new(),
            jig: "Unknown Jig".to_string(),
            scenario: "Unknown Scenario".to_string(),
            interfaces: HashMap::new(),
            controller: controller.clone(),
        }));

        controller.lock().unwrap().set_testset(test_set.clone());

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

        test_set.lock().unwrap().load_jigs(&jig_paths);
        test_set.lock().unwrap().load_loggers(&logger_paths);
        test_set.lock().unwrap().load_interfaces(&interface_paths);
        //test_set.load_services(&service_paths);
        //test_set.load_updaters(&updater_paths);
        //test_set.load_tests(&test_paths);
        //test_set.load_scenarios(&scenario_paths);
        //test_set.load_triggers(&trigger_paths);
        //test_set.load_coupons(&coupon_paths);

        Ok(test_set)
    }

    pub fn debug(&self, unit_type: &str, unit: &str, msg: &str) {
        let now = time::SystemTime::now();
        let elapsed = match now.duration_since(time::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => time::Duration::new(0, 0),
        };

        self.controller.lock().unwrap().control_message(&Message {
            message_type: 2,
            unit: unit.to_string(),
            unit_type: unit_type.to_string(),
            unix_time: elapsed.as_secs(),
            unix_time_nsecs: elapsed.subsec_nanos(),
            message: MessageContents::Log(msg.to_string()),
        });
    }

    fn load_jigs(&mut self, jig_paths: &Vec<PathBuf>) {
        for jig_path in jig_paths {
            let item_name = jig_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = jig_path.to_str().unwrap_or("");

            let new_jig = Jig::new(&self, item_name, path_str);

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
            self.jig = new_jig.id().clone();
            self.jigs.insert(new_jig.id().clone(), new_jig);
        }
    }

    fn load_loggers(&mut self, logger_paths: &Vec<PathBuf>) {
        for logger_path in logger_paths {
            let item_name = logger_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = logger_path.to_str().unwrap_or("");
            let new_logger = Logger::new(&self, item_name, path_str, &self.jigs);

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
            new_logger.start(&self);
            self.loggers.insert(new_logger.id().clone(), new_logger);
        }
    }

    pub fn monitor_logs<F>(&self, logger_func: F)
        where F: Send + 'static + Fn(Message) {
        self.controller.lock().unwrap().deref_mut().add_logger(logger_func);
    }

    pub fn monitor_broadcasts<F>(&self, broadcast_func: F)
        where F: Send + 'static + Fn(Message) {
        self.controller.lock().unwrap().deref_mut().add_broadcast(broadcast_func);
    }

    fn load_interfaces(&mut self, interface_paths: &Vec<PathBuf>) {
        for interface_path in interface_paths {
            let item_name = interface_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = interface_path.to_str().unwrap_or("");
            let new_interface = match Interface::new(&self, item_name, path_str, &self.jigs, self.controller.clone()) {
                // In this case, it just means the interface is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => { self.debug("interface", item_name, format!("Unable to load interface: {:?}", e).as_str()); continue; },
                        Ok(s) => s,
                    }
                },
            };

            let id = new_interface.id().clone();
            new_interface.start(&self);
            self.interfaces.insert(new_interface.id().clone(), new_interface);
        }
    }

    fn resolve_scenarios(&mut self) {
        for (_, ref mut scenario) in self.scenarios.iter_mut() {
            scenario.resolve_tests(&self.tests);
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

    pub fn get_jig(&self) -> String {
        self.jig.clone()
    }

    pub fn get_jig_name(&self) -> String {
        self.jigs[self.jig.deref()].name().clone()
    }

    pub fn get_jig_description(&self) -> String {
        self.jigs[self.jig.deref()].description().clone()
    }

    pub fn get_controller(&self) -> Arc<Mutex<controller::Controller>> {
        return self.controller.clone();
    }

    pub fn set_scenario(&mut self, scenario_name: &String) {
        let scenario = match self.scenarios.get(scenario_name) {
            None => {
                self.debug(self.unit_type(), self.unit_name(), format!("Unable to find scenario: {}", scenario_name).as_str());
                return;
            },
            Some(s) => s,
        };
        self.scenario = scenario_name.clone();
        scenario.describe();
    }

    pub fn unit_type(&self) -> &'static str {
        "internal"
    }

    pub fn unit_name(&self) -> &'static str {
        "testset"
    }
}
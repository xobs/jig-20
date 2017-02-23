extern crate bus;

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
use std::fmt;
use std::io::Error;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex, mpsc};
use std::ops::{DerefMut, Deref};

use super::controller::{BroadcastMessage, BroadcastMessageContents, ControlMessageContents, Controller};

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
pub struct TestSet {
    tests: HashMap<String, Arc<Mutex<Test>>>,
    scenarios: HashMap<String, Arc<Mutex<Scenario>>>,
    triggers: HashMap<String, Arc<Mutex<Trigger>>>,
    loggers: HashMap<String, Arc<Mutex<Logger>>>,
    jigs: HashMap<String, Arc<Mutex<Jig>>>,
    interfaces: HashMap<String, Arc<Mutex<Interface>>>,

    /// The jig that we've decided to use.
    jig: Option<Arc<Mutex<Jig>>>,

    /// The id of the scenario that we're using.
    scenario: Option<Arc<Mutex<Scenario>>>,

    /*
    coupons: HashMap<String, Coupon>,
    updaters: HashMap<String, Updater>,
    services: HashMap<String, Service>,
    */

    controller: controller::Controller,

    /// A sender, used for sending Control messages.
    control: mpsc::Sender<controller::ControlMessage>,

    /// A broadcast bus, used for sending broadcast messages.
    broadcast: Arc<Mutex<bus::Bus<controller::BroadcastMessage>>>,
}

impl fmt::Debug for TestSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[TestSet]")
    }
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(dir: &str, controller: &mut controller::Controller) -> Result<Arc<Mutex<TestSet>>, Error> {

        let test_set = Arc::new(Mutex::new(TestSet {
            tests: HashMap::new(),
            scenarios: HashMap::new(),
            loggers: HashMap::new(),
            triggers: HashMap::new(),
            jigs: HashMap::new(),
            jig: None,
            scenario: None,
            interfaces: HashMap::new(),
            controller: controller.clone(),
            control: controller.new_control(),
            broadcast: controller.new_broadcast(),
        }));

        // Add a simple logger to show us debug data.
        Controller::add_logger(&test_set.lock().unwrap().broadcast, |msg| println!("DEBUG>> {:?}", msg));

        controller.set_testset(test_set.clone());

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
                unknown => println!("Unrecognized unit type {}, path: {}", unknown, path.to_str().unwrap_or("")),
            }
        }

        test_set.lock().unwrap().load_jigs(&jig_paths);
        test_set.lock().unwrap().load_loggers(&logger_paths);
        test_set.lock().unwrap().load_interfaces(&interface_paths);
        //test_set.load_services(&service_paths);
        //test_set.load_updaters(&updater_paths);
        test_set.lock().unwrap().load_tests(&test_paths);
        test_set.lock().unwrap().load_scenarios(&scenario_paths);
        //test_set.load_triggers(&trigger_paths);
        //test_set.load_coupons(&coupon_paths);

        Ok(test_set)
    }

    pub fn debug(&self, unit_type: &str, unit_id: &str, msg: &str) {
        Controller::control_class(&self.control,
                                  "debug",
                                  unit_id,
                                  unit_type,
                                  &ControlMessageContents::Log(msg.to_string()));
    }

    fn load_jigs(&mut self, jig_paths: &Vec<PathBuf>) {
        for jig_path in jig_paths {
            let item_name = jig_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = jig_path.to_str().unwrap_or("");

            let new_jig = match Jig::new(&self, item_name, path_str, &self.controller) {
                // The jig will return "None" if it is incompatible.
                None => continue,
                Some(s) => s,
            };

            let new_jig = match new_jig {
                Err(e) => {println!("Unable to load jig file: {:?}", e); continue;},
                Ok(s) => Arc::new(Mutex::new(s)),
            };

            let new_jig_id = new_jig.lock().unwrap().id().to_string();

            if self.jig.is_none() {
                self.jig = Some(new_jig.clone());
            }

            self.jigs.insert(new_jig_id, new_jig);
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
            match new_logger.start(&self) {
                Err(e) => {println!("Unable to start logger: {}", e);},
                Ok(_) => (),
            }
            self.loggers.insert(new_logger.id().to_string(), Arc::new(Mutex::new(new_logger)));
        }
    }

    pub fn monitor_logs<F>(&self, logger_func: F)
        where F: Send + 'static + Fn(BroadcastMessage) {
        Controller::add_logger(&self.broadcast, logger_func);
    }

    pub fn monitor_broadcasts<F>(&self, broadcast_func: F)
        where F: Send + 'static + Fn(BroadcastMessage) {
        Controller::add_broadcast(&self.broadcast, broadcast_func);
    }

    fn load_interfaces(&mut self, interface_paths: &Vec<PathBuf>) {
        for interface_path in interface_paths {
            let item_name = interface_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = interface_path.to_str().unwrap_or("");
            let new_interface = match Interface::new(&self, item_name, path_str, &self.jigs, &self.controller) {
                // In this case, it just means the interface is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => { self.debug("interface", item_name, format!("Unable to load interface: {:?}", e).as_str()); continue; },
                        Ok(s) => s,
                    }
                },
            };

            match new_interface.start(&self) {
                Err(e) => {println!("Unable to start interface: {}", e);},
                Ok(_) => (),
            }

            self.interfaces.insert(new_interface.id().to_string(), Arc::new(Mutex::new(new_interface)));
        }
        if let Some(ref jig) = self.jig.as_ref() {
            jig.lock().unwrap().describe();
        }
    }

    fn load_tests(&mut self, test_paths: &Vec<PathBuf>) {
        for test_path in test_paths {
            let item_name = test_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = test_path.to_str().unwrap_or("");
            let new_test = match Test::new(&self,
                                           item_name,
                                           path_str,
                                           &self.jigs,
                                           &self.control,
                                           &self.broadcast) {
                // In this case, it just means the test is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => { self.debug("test", item_name, format!("Unable to load test: {:?}", e).as_str()); continue; },
                        Ok(s) => s,
                    }
                },
            };

            new_test.describe();
            self.tests.insert(new_test.id().to_string(), Arc::new(Mutex::new(new_test)));
        }
    }

    fn load_scenarios(&mut self, paths: &Vec<PathBuf>) {
        let default_scenario_name = self.get_jig_default_scenario().clone();
        for path in paths {
            let item_name = path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = path.to_str().unwrap_or("");
            let new_scenario = match Scenario::new(&self,
                                                   item_name,
                                                   path_str,
                                                   &self.jigs,
                                                   &self.tests,
                                                   &self.control,
                                                   &self.broadcast) {
                // In this case, it just means the test is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => { self.debug("scenario", item_name, format!("Unable to load scenario: {:?}", e).as_str()); continue; },
                        Ok(s) => s,
                    }
                },
            };

            let new_scenario_id = new_scenario.id().to_string();
            let new_scenario = Arc::new(Mutex::new(new_scenario));

            self.scenarios.insert(new_scenario_id.clone(), new_scenario);

            if let Some(default_name) = default_scenario_name.clone() {
                if new_scenario_id == default_name {
                    self.set_scenario(&new_scenario_id);
                }
            }
        }

        self.send_scenarios();
        for (_, scenario) in self.scenarios.iter() {
            scenario.lock().unwrap().deref_mut().describe();
        }
        // If a default test has been selected, send the tests.
        self.send_tests(None);
    }

    pub fn get_jig_default_scenario(&self) -> Option<String> {
        match self.jig.as_ref() {
            None => None,
            Some(s) => {
                let jig = s.lock().unwrap();
                jig.default_scenario().clone()
            }
        }
    }

    pub fn get_jig_id(&self) -> String {
        match self.jig.as_ref() {
            None => "".to_string(),
            Some(s) => {
                let jig = s.lock().unwrap();
                jig.id().to_string()
            }
        }
    }

    pub fn get_jig_name(&self) -> String {
        match self.jig.as_ref() {
            None => "".to_string(),
            Some(s) => {
                let jig = s.lock().unwrap();
                jig.name().to_string()
            }
        }
    }

    pub fn get_jig_description(&self) -> String {
        match self.jig.as_ref() {
            None => "".to_string(),
            Some(s) => {
                let jig = s.lock().unwrap();
                jig.description().to_string()
            }
        }
    }

    pub fn advance_scenario(&self) {
        // Unwrap, because if it is None then things are very broken.
        match self.scenario {
            None => panic!("self.scenario was None"),
            Some(ref s) => {
                let ref scenario = s.lock().unwrap();
                scenario.advance();
            },
        };
    }

    pub fn start_scenario(&mut self, scenario_id: Option<String>) {

        // Figure out what scenario to run.  Run the default scenario if unspecified.
        let scenario: Arc<Mutex<Scenario>> = match scenario_id {
            None => match self.scenario {
                None => {
                    self.debug(self.unit_type(), self.unit_name(), format!("No default scenario selected").as_str());
                    return;
                },
                Some(ref t) => t.clone(),
            },
            Some(s) => self.scenarios[s.as_str()].clone(),
        };

        // Store the scenario that we're running into the testset.
        self.scenario = Some(scenario.clone());

        // Unlock the scenario so we have exclusive access to it.
        let ref scenario = scenario.lock().unwrap();

        // Start the scenario with the jig's default working directory.
        let working_directory = match self.jig {
            None => None,
            Some(ref jig) => (jig.lock().unwrap().default_working_directory()).clone(),
        };

        scenario.start(&working_directory);
    }

    pub fn send_scenarios(&self) {
        let scenario_list = self.scenarios.values().map(|x| x.lock().unwrap().deref().id().to_string()).collect();

        Controller::broadcast(&self.broadcast,
                              self.unit_name(),
                              self.unit_type(),
                              &BroadcastMessageContents::Scenarios(scenario_list));
    }

    pub fn send_tests(&self, scenario_id: Option<String>) {
        let ref scenario = match scenario_id {
            None => match self.scenario {
                None => {self.debug(self.unit_type(), self.unit_name(), format!("No default scenario selected").as_str()); return;},
                Some(ref t) => t.lock().unwrap(),
            },
            Some(s) => self.scenarios[s.as_str()].lock().unwrap(),
        };
        scenario.describe();
    }

    pub fn set_interface_hello(&self, id: String, hello: String) {
        match self.interfaces.get(&id) {
            None => return,
            Some(s) => s.lock().unwrap().set_hello(hello),
        }
    }

    pub fn set_scenario(&mut self, scenario_name: &String) {
        let scenario = match self.scenarios.get(scenario_name) {
            None => {
                self.debug(self.unit_type(), self.unit_name(), format!("Unable to find scenario: {}", scenario_name).as_str());
                return;
            },
            Some(s) => s,
        };
        self.scenario = Some(scenario.clone());

        Controller::broadcast(&self.broadcast,
                              self.unit_name(),
                              self.unit_type(),
                              &BroadcastMessageContents::Scenario(scenario_name.clone()));
        scenario.lock().unwrap().deref_mut().describe();
    }

    pub fn unit_type(&self) -> &'static str {
        "internal"
    }

    pub fn unit_name(&self) -> &'static str {
        "testset"
    }
}
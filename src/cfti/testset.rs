extern crate termcolor;

use cfti::config;
use cfti::controller::{self, ControlMessageContents};
use cfti::types::{Test, Scenario, Logger, Trigger, Jig, Interface};
use cfti::types::unit::Unit;
// use cfti::types::Coupon;
// use cfti::types::Updater;
// use cfti::types::Service;
//
use cfti::unitfile::UnitFile;

use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver};
use std::ops::{DerefMut, Deref};

use super::controller::{BroadcastMessageContents, Controller};

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
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

    /// Tests can "Provide" other tests.  This maps those.
    test_aliases: HashMap<String, String>,

    // coupons: HashMap<String, Coupon>,
    // updaters: HashMap<String, Updater>,
    // services: HashMap<String, Service>,
    //
    /// The controller object, where messages come and go.
    controller: Controller,

    /// A control channel from the Controller, to manipulate the testset
    receiver: Receiver<TestSetCommand>,

    /// The current system configuration
    config: config::Config,
}

#[derive(Debug)]
pub enum TestSetError {
    TestSetIoError(Error),
}

impl From<Error> for TestSetError {
    fn from(kind: Error) -> Self {
        TestSetError::TestSetIoError(kind)
    }
}

#[derive(Debug)]
pub enum TestSetCommand {
    DescribeJig,
    AbortScenario,
    SetScenario(String),
    SetInterfaceHello(String, String),
    StartScenario(Option<String>),
    AdvanceScenario,
    AbortTests,
    SendScenarios,
    SendTests(Option<String>),
    Shutdown,
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(controller: &mut controller::Controller, config: config::Config) -> TestSet {

        let (sender, receiver) = channel();

        let test_set = TestSet {
            tests: HashMap::new(),
            test_aliases: HashMap::new(),
            scenarios: HashMap::new(),
            loggers: HashMap::new(),
            triggers: HashMap::new(),
            jigs: HashMap::new(),
            jig: None,
            scenario: None,
            interfaces: HashMap::new(),
            controller: controller.clone(),
            receiver: receiver,
            config: config,
        };

        test_set.control(ControlMessageContents::SetTestsetChannel(sender));
        test_set
    }

    pub fn add_dir(&mut self, dir: &str) -> Result<(), TestSetError> {

        let entries_rd: fs::ReadDir = try!(fs::read_dir(dir));
        for entry_opt in entries_rd {
            let entry = try!(entry_opt);
            let path = entry.path();
            if !try!(entry.file_type()).is_file() {
                continue;
            }

            let unit_id = path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let unit_type = path.extension().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let unitfile = match UnitFile::new(path.to_str().unwrap_or("")) {
                Err(e) => {
                    self.config_error(unit_id,
                                      unit_type,
                                      format!("Unable to load unit file: {:?}", e));
                    continue;
                }
                Ok(s) => s,
            };

            self.load_unit(unit_id, unit_type, unitfile);

        }
        Ok(())
    }

    fn load_unit(&mut self, unit_id: &str, unit_type: &str, unitfile: UnitFile) {

        match unit_type {
            "jig" => self.load_jig(unitfile, unit_id),
            //            "logger" => self.load_logger(path, config),
            //            "interface" => self.load_interface(path, config),
            //            "service" => self.load_service(path, config),
            //            "updater" => self.load_updater(path, config),
            //            "test" => self.load_test(path, config),
            //            "scenario" => self.load_scenario(path, config),
            //            "trigger" => self.load_trigger(path, config),
            //            "coupon" => self.load_coupon(path, config),
            unit_type => {
                self.config_error(unit_id,
                                  unit_type,
                                  format!("Unrecognized unit type: {}", unit_type));
            }
        }
    }

    fn load_jig(&mut self, unit_file: UnitFile, unit_id: &str) {

        let new_jig = match Jig::new(unit_id, unit_file, self) {
            // The jig will return "None" if it is incompatible.
            None => return,
            Some(Ok(s)) => Arc::new(Mutex::new(s)),
            Some(Err(e)) => {
                self.config_error(unit_id, "jig", format!("Unable to load jig: {:?}", e));
                return;
            }
        };

        // if self.jig.is_none() {
        //    self.jig = Some(new_jig.clone());
        // }

        self.jigs.insert(unit_id.to_string(), new_jig);
    }

    fn load_loggers(&mut self, config: &config::Config, logger_paths: &Vec<PathBuf>) {
        // Start the scenario with the jig's default working directory.
        let working_directory = match self.jig {
            None => None,
            Some(ref jig) => (jig.lock().unwrap().default_working_directory()).clone(),
        };

        for logger_path in logger_paths {
            let item_name =
                logger_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = logger_path.to_str().unwrap_or("");
            let new_logger_res =
                Logger::new(item_name, path_str, &self.jigs, config, &self.controller);

            // In this case, it just means the logger is incompatible.
            let new_logger = match new_logger_res {
                None => continue,
                // If there was an error loading the logger, note it and continue.
                Some(s) => {
                    match s {
                        Err(e) => {
                            self.debug(format!("Unable to load logger {}: {:?}", item_name, e));
                            continue;
                        }
                        Ok(t) => t,
                    }
                }
            };

            // If the new logger fails to start, ignore it and move on.
            if let Err(e) = new_logger.start(&working_directory) {
                self.debug(format!("Unable to start logger {}: {:?}", new_logger.id(), e));
                continue;
            };
            self.loggers.insert(new_logger.id().to_string(),
                                Arc::new(Mutex::new(new_logger)));
        }
    }

    fn load_interfaces(&mut self, config: &config::Config, interface_paths: &Vec<PathBuf>) {

        // Start the trigger with the jig's default working directory.
        let working_directory = match self.jig {
            None => None,
            Some(ref jig) => (jig.lock().unwrap().default_working_directory()).clone(),
        };

        for interface_path in interface_paths {
            let item_name =
                interface_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = interface_path.to_str().unwrap_or("");
            let new_interface = match Interface::new(item_name, path_str, self, config) {
                // In this case, it just means the interface is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => {
                            self.debug(format!("Unable to load interface {}: {:?}", item_name, e));
                            continue;
                        }
                        Ok(s) => s,
                    }
                }
            };

            match new_interface.start(&working_directory) {
                Err(e) => {
                    self.debug(format!("Unable to start interface {}: {:?}",
                                       new_interface.id(),
                                       e));
                    continue;
                }
                Ok(_) => (),
            }

            self.interfaces.insert(new_interface.id().to_string(),
                                   Arc::new(Mutex::new(new_interface)));
        }
        if let Some(ref jig) = self.jig.as_ref() {
            jig.lock().unwrap().describe();
        }
    }

    fn load_triggers(&mut self, config: &config::Config, trigger_paths: &Vec<PathBuf>) {

        // Start the trigger with the jig's default working directory.
        let working_directory = match self.jig {
            None => None,
            Some(ref jig) => (jig.lock().unwrap().default_working_directory()).clone(),
        };

        for trigger_path in trigger_paths {
            let item_name =
                trigger_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = trigger_path.to_str().unwrap_or("");
            let new_trigger = match Trigger::new(item_name, path_str, &self, config) {
                // In this case, it just means the interface is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => {
                            self.debug(format!("Unable to load trigger {}: {:?}", item_name, e));
                            continue;
                        }
                        Ok(s) => s,
                    }
                }
            };

            match new_trigger.start(&working_directory) {
                Err(e) => {
                    self.debug(format!("Unable to start trigger {}: {:?}", new_trigger.id(), e));
                    continue;
                }
                Ok(_) => (),
            }

            self.triggers.insert(new_trigger.id().to_string(),
                                 Arc::new(Mutex::new(new_trigger)));
        }
    }

    fn load_tests(&mut self, config: &config::Config, test_paths: &Vec<PathBuf>) {
        for test_path in test_paths {
            let item_name = test_path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = test_path.to_str().unwrap_or("");
            let new_test = match Test::new(item_name, path_str, self, config) {
                // In this case, it just means the test is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => {
                            self.warn(format!("Unable to load test {}: {:?}", item_name, e));
                            continue;
                        }
                        Ok(s) => s,
                    }
                }
            };

            // If another test already Provides this one, complain.
            if let Some(collision) = self.test_aliases.get(&new_test.id().to_string()) {
                self.warn(format!("Error: Loaded test {}, but test {} already 'Provides'",
                                  new_test.id(),
                                  collision));
                continue;
            }

            for test_provides in new_test.provides() {
                if let Some(collision) = self.test_aliases.get(test_provides) {
                    self.warn(format!("Error: Loaded test {}, but both it and test {} \
                                        'Provides' {}",
                                      new_test.id(),
                                      collision,
                                      test_provides));
                    continue;
                }
            }

            // Now that we know we're unique, add the alises.
            self.test_aliases.insert(new_test.id().to_string(), new_test.id().to_string());
            for test_provides in new_test.provides() {
                self.test_aliases.insert(test_provides.clone(), new_test.id().to_string());
            }

            new_test.describe();
            self.tests.insert(new_test.id().to_string(), Arc::new(Mutex::new(new_test)));
        }
    }

    fn load_scenarios(&mut self, config: &config::Config, paths: &Vec<PathBuf>) {
        let default_scenario_name = self.get_jig_default_scenario().clone();
        for path in paths {
            let item_name = path.file_stem().unwrap_or(OsStr::new("")).to_str().unwrap_or("");
            let path_str = path.to_str().unwrap_or("");
            let new_scenario = match Scenario::new(item_name, path_str, self, config) {
                // In this case, it just means the test is incompatible.
                None => continue,
                Some(s) => {
                    match s {
                        Err(e) => {
                            self.debug(format!("Unable to load scenario {}: {:?}", item_name, e));
                            continue;
                        }
                        Ok(s) => s,
                    }
                }
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

    pub fn config(&self) -> &config::Config {
        &self.config
    }

    fn config_error(&self, unit_id: &str, unit_type: &str, msg: String) {
        self.controller().broadcast_class("config-error",
                                          unit_id,
                                          unit_type,
                                          &BroadcastMessageContents::Log(msg));
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

    pub fn describe_jig(&self) {
        if let Some(ref j) = self.jig {
            self.broadcast(BroadcastMessageContents::Jig(j.lock().unwrap().id().to_string()));
            j.lock().unwrap().describe();
        }
    }

    pub fn advance_scenario(&self) {
        // Unwrap, because if it is None then things are very broken.
        match self.scenario {
            None => panic!("self.scenario was None"),
            Some(ref s) => {
                let ref scenario = s.lock().unwrap();
                scenario.advance();
            }
        };
    }

    pub fn abort_scenario(&self) {
        // If there is no scenario, that's fine.  There's nothing to do.
        if let Some(ref s) = self.scenario {
            s.lock().unwrap().abort();
        }
    }

    pub fn start_scenario(&mut self, scenario_id: Option<String>) {

        // Figure out what scenario to run.  Run the default scenario if unspecified.
        let scenario: Arc<Mutex<Scenario>> = match scenario_id {
            None => {
                match self.scenario {
                    None => {
                        self.debug(format!("No default scenario selected"));
                        return;
                    }
                    Some(ref t) => t.clone(),
                }
            }
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
        let scenario_list =
            self.scenarios.values().map(|x| x.lock().unwrap().deref().id().to_string()).collect();
        self.broadcast(BroadcastMessageContents::Scenarios(scenario_list));
    }

    pub fn send_tests(&self, scenario_id: Option<String>) {
        let ref scenario = match scenario_id {
            None => {
                match self.scenario {
                    None => {
                        self.debug(format!("No default scenario selected"));
                        return;
                    }
                    Some(ref t) => t.lock().unwrap(),
                }
            }
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
                self.debug(format!("Unable to find scenario: {}", scenario_name));
                return;
            }
            Some(s) => s,
        };
        self.scenario = Some(scenario.clone());

        self.broadcast(BroadcastMessageContents::Scenario(scenario_name.clone()));
        scenario.lock().unwrap().deref_mut().describe();
    }

    pub fn run(&mut self) {
        loop {
            let msg = match self.receiver.recv() {
                Ok(o) => o,
                Err(e) => {
                    println!("Received error: {:?}", e);
                    return;
                }
            };

            match msg {
                TestSetCommand::DescribeJig => self.describe_jig(),
                TestSetCommand::AbortScenario => self.abort_scenario(),
                TestSetCommand::SetScenario(new_scenario) => self.set_scenario(&new_scenario),
                TestSetCommand::SetInterfaceHello(id, msg) => self.set_interface_hello(id, msg),
                TestSetCommand::StartScenario(optional_name) => self.start_scenario(optional_name),
                TestSetCommand::AdvanceScenario => self.advance_scenario(),
                TestSetCommand::AbortTests => self.abort_scenario(),
                TestSetCommand::SendScenarios => self.send_scenarios(),
                TestSetCommand::SendTests(optional_name) => self.send_tests(optional_name),
                TestSetCommand::Shutdown => return,
            }
        }
    }

    pub fn jigs(&self) -> &HashMap<String, Arc<Mutex<Jig>>> {
        &self.jigs
    }

    pub fn tests(&self) -> &HashMap<String, Arc<Mutex<Test>>> {
        &self.tests
    }
}

impl Unit for TestSet {
    fn id(&self) -> &str {
        "testset"
    }

    fn kind(&self) -> &str {
        "testset"
    }

    fn name(&self) -> &str {
        "testset"
    }

    fn description(&self) -> &str {
        "Primary collection of all objects in this test"
    }

    fn controller(&self) -> &Controller {
        &self.controller
    }
}

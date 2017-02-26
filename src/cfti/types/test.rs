extern crate ini;
extern crate bus;
extern crate regex;

use self::ini::Ini;
use self::regex::Regex;

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time;

use cfti::types::Jig;
use cfti::controller::{Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::process;

#[derive(Debug)]
pub enum TestError {
    FileLoadError,
    MissingTestSection,
    MissingExecSection,
    InvalidType(String),
    DaemonReadyTextError,
}

#[derive(Debug)]
enum TestType {
    Simple,
    Daemon,
}

#[derive(Debug)]
pub struct Test {

    /// Id: File name on disk, what other units refer to this one as.
    id: String,

    /// Name: Defines the short name for this test.
    name: String,

    /// Description: Defines a detailed description of this test.  May be up to one paragraph.
    description: String,

    /// Requires: The name of a test that must successfully complete
    requires: Vec<String>,

    /// Suggests: The name of a test that should be run first, but is not catastrophic if it fails
    suggests: Vec<String>,

    /// Provides is a list of tests that this can be referred to when "Requiring" or "Suggesting" tests.
    provides: Vec<String>,

    /// Timeout: The maximum number of seconds that this test may be run for.
    timeout: u64,

    /// Type: One of "simple" or "daemon".  For "simple" tests, the return code will indicate pass or fail,
    /// and each line printed will be considered progress.  For "daemon", the process will be forked
    /// and left to run in the background.  See "daemons" in the documentation.
    test_type: TestType,

    /// A regex that can be used to determine if a test is ready.
    test_daemon_ready: Option<Regex>,

    /// ExecStart: The command to run as part of this test.
    exec_start: String,

    /// ExecStopFail: When stopping tests, if the test failed, then this stop command will be run.
    exec_stop_failure: Option<String>,

    /// ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
    exec_stop_success: Option<String>,

    /// The controller where messages come and go.
    controller: Controller,

    /// The last line outputted by a test, which is the result.
    last_line: Arc<Mutex<String>>,

    /// Whether the last run of this test succeeded or not.
    last_result: Arc<Mutex<bool>>,

    /// The currently-running test process.  Particularly important for daemons.
    test_process: Arc<Mutex<Option<process::Process>>>,
}

impl Test {
    pub fn new(id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               controller: &Controller) -> Option<Result<Test, TestError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(TestError::FileLoadError)),
            Ok(s) => s,
        };

        let test_section = match ini_file.section(Some("Test")) {
            None => return Some(Err(TestError::MissingTestSection)),
            Some(s) => s,
        };

        // Check to see if this test is compatible with this jig.
        match test_section.get("Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> = s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break
                    }
                }
                if found_it == false {
                    controller.debug("test", id, format!("The test '{}' is not compatible with this jig", id));
                    return None;
                }
            }
        }

        let test_daemon_ready = match test_section.get("DaemonReadyText") {
            None => None,
            Some(s) => match Regex::new(s) {
                Ok(o) => Some(o),
                Err(e) => {
                    controller.debug("test", id, format!("Unable to compile DaemonReadyText: {}", e));
                    return Some(Err(TestError::DaemonReadyTextError));
                },
            },
        };

        let test_type = match test_section.get("Type") {
            None => TestType::Simple,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "simple" => TestType::Simple,
                "daemon" => TestType::Daemon,
                other => return Some(Err(TestError::InvalidType(other.to_string()))),
            },
        };

        let exec_start = match test_section.get("ExecStart") {
            None => return Some(Err(TestError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let exec_stop_success = match test_section.get("ExecStopSuccess") {
            None => match test_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_failure = match test_section.get("ExecStopFail") {
            None => match test_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let description = match test_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match test_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let timeout = match test_section.get("Timeout") {
            None => 2000,
            Some(s) => s.parse().unwrap(),
        };

        // Get a list of all the requirements, or make a blank list
        let requires = match test_section.get("Requires") {
            None => Vec::new(),
            // Split by "," and also whitespace, and combine back into an array.
            Some(s) => s.split(",").map(|x|
                        x.to_string().split_whitespace().map(|y|
                        y.to_string().trim().to_string()).collect()).collect()
        };

        let suggests = match test_section.get("Suggests") {
            None => Vec::new(),
            // Split by "," and also whitespace, and combine back into an array.
            Some(s) => s.split(",").map(|x|
                        x.to_string().split_whitespace().map(|y|
                        y.to_string().trim().to_string()).collect()).collect()
        };

        let provides = match test_section.get("Provides") {
            None => vec![],
            Some(s) =>
                s.split(",").map(|x|
                        x.to_string().split_whitespace().map(|y|
                y.to_string().trim().to_string()).collect()).collect()
        };

        Some(Ok(Test {
            id: id.to_string(),
            name: name,
            description: description,

            requires: requires,
            suggests: suggests,
            provides: provides,

            test_type: test_type,
            test_daemon_ready: test_daemon_ready,
            test_process: Arc::new(Mutex::new(None)),

            timeout: timeout,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,

            controller: controller.clone(),

            last_line: Arc::new(Mutex::new("".to_string())),
            last_result: Arc::new(Mutex::new(false)),
        }))
    }

    pub fn describe(&self) {
        self.controller.broadcast(
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "name".to_string(),
                                                                  self.id().to_string(),
                                                                  self.name().to_string()));
        self.controller.broadcast(
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "description".to_string(),
                                                                  self.id().to_string(),
                                                                  self.description().to_string()));
    }

    pub fn timeout(&self) -> u64 {
        self.timeout
    }

    /// Start running a test
    ///
    /// Start running a test.  If `working_directory` is specified and
    /// there is no WorkingDirectory in this test, use the provided one.
    pub fn start(&self, working_directory: &Option<String>, max_duration: time::Duration) {
        self.broadcast(BroadcastMessageContents::Running(self.id().to_string()));

        match self.test_type {
            TestType::Simple => self.start_simple(working_directory, max_duration),
            TestType::Daemon => self.start_daemon(working_directory, max_duration),
        }
    }

    fn start_daemon(&self, working_directory: &Option<String>, max_duration: time::Duration) {
        ;
    }

    fn start_simple(&self, working_directory: &Option<String>, max_duration: time::Duration) {
        // Try to create a command.  If this fails, then the command completion will be called,
        // so we can just ignore the error.
        let controller = self.controller.clone();
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        let cmd = self.exec_start.clone();
        let last_line = self.last_line.clone();
        let result = self.last_result.clone();
        let process = match process::try_command_completion(
                        cmd.as_str(),
                        working_directory,
                        max_duration,
                        move |res: Result<(), process::CommandError>| {
            let msg = match res {
                Ok(_) => {
                    *(result.lock().unwrap()) = true;
                    BroadcastMessageContents::Pass(id.clone(), last_line.lock().unwrap().to_string())
                },
                Err(e) => {
                    *(result.lock().unwrap()) = false;
                    BroadcastMessageContents::Fail(id.clone(), format!("{:?}", e))
                },
            };

            // Send a message indicating what the test did, and advance the scenario.
            controller.broadcast_class("result", id.as_str(), kind.as_str(), &msg);
            controller.control_class(
                "result",
                id.as_str(),
                kind.as_str(),
                &ControlMessageContents::AdvanceScenario);
        }) {
            Err(_) => return,
            Ok(o) => o,
        };

        let thr_last_line = self.last_line.clone();
        let thr_controller = self.controller.clone();
        let thr_id = self.id().to_string();
        let thr_kind = self.kind().to_string();
        process::watch_output(process.stdout, &self.controller, self.id(), self.kind(),
            move |msg| {
                *(thr_last_line.lock().unwrap()) = msg.clone();
                thr_controller.broadcast_class(
                            "stdout",
                            thr_id.as_str(),
                            thr_kind.as_str(),
                            &BroadcastMessageContents::Log(msg)
                );
                Ok(())
            });

        let thr_last_line = self.last_line.clone();
        let thr_controller = self.controller.clone();
        let thr_id = self.id().to_string();
        let thr_kind = self.kind().to_string();
        process::watch_output(process.stderr, &self.controller, self.id(), self.kind(),
            move |msg| {
                *(thr_last_line.lock().unwrap()) = msg.clone();
                thr_controller.broadcast_class(
                            "stderr",
                            thr_id.as_str(),
                            thr_kind.as_str(),
                            &BroadcastMessageContents::Log(msg)
                );
                Ok(())
            });

    }

    pub fn stop(&self) {
        match *self.last_result.lock().unwrap() {
            true => if let Some(ref cmd) = self.exec_stop_success {
                self.log(format!("Running success: {}", cmd));
            },
            false => if let Some(ref cmd) = self.exec_stop_failure {
                self.log(format!("Running failure: {}", cmd));
            },
        };
    }

    pub fn broadcast(&self, msg: BroadcastMessageContents) {
        self.controller.broadcast(self.id(), self.kind(), &msg);
    }

    pub fn log(&self, msg: String) {
        self.broadcast(BroadcastMessageContents::Log(msg));
    }

    pub fn requirements(&self) -> &Vec<String> {
        &self.requires
    }

    pub fn suggestions(&self) -> &Vec<String> {
        &self.suggests
    }

    pub fn provides(&self) -> &Vec<String> {
        &self.provides
    }

    pub fn kind(&self) -> &str {
        "test"
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn description(&self) -> &str {
        self.description.as_str()
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }
}
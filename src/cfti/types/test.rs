extern crate bus;
extern crate regex;

use self::regex::Regex;

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time;
use std::thread;
use std::io::{self, BufRead};

use cfti::types::Jig;
use cfti::controller::{Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::process;
use cfti::config;
use cfti::unitfile::UnitFile;

#[derive(Debug)]
pub enum TestError {
    FileLoadError(String),
    MissingTestSection,
    MissingExecSection,
    ParseTimeoutError,
    InvalidType(String),
    DaemonReadyTextError,
}

#[derive(Debug, PartialEq)]
enum TestType {
    Simple,
    Daemon,
}

#[derive(Debug, PartialEq)]
pub enum TestState {

    /// A test has yet to be run.
    Pending,

    /// A daemon is waiting for its "match" text to appear.
    Starting,

    /// A test (or daemon) is in the process of running.
    Running,

    /// A test (or daemon) passed successfully.
    Pass,

    /// A test (or daemon) failed for some reason.
    Fail(String),
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
    timeout: time::Duration,

    /// The maximum amount of time to allow an ExecStopSuccess to run
    exec_stop_success_timeout: time::Duration,

    /// The maximum amount of time to allow an ExecStopFailure to run
    exec_stop_failure_timeout: time::Duration,

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

    /// working_directory: Directory to run progrms from
    working_directory: Option<String>,

    /// The controller where messages come and go.
    controller: Controller,

    /// The last line outputted by a test, which is the result.
    last_line: Arc<Mutex<String>>,

    /// Whether the last run of this test succeeded or not.
    state: Arc<Mutex<TestState>>,

    /// The currently-running test process.  Particularly important for daemons.
    test_process: Arc<Mutex<Option<process::ChildProcess>>>,

    /// The working directory for the current test.
    test_working_directory: Arc<Mutex<Option<String>>>,
}

impl Test {
    pub fn new(id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               config: &config::Config,
               controller: &Controller) -> Option<Result<Test, TestError>> {

        // Load the .ini file
        let unitfile = match UnitFile::new(path) {
            Err(e) => return Some(Err(TestError::FileLoadError(format!("{:?}", e)))),
            Ok(s) => s,
        };

        if ! unitfile.has_section("Test") {
            return Some(Err(TestError::MissingTestSection));
        }

        // Check to see if this test is compatible with this jig.
        match unitfile.get("Test", "Jigs") {
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

        let test_daemon_ready = match unitfile.get("Test", "DaemonReadyText") {
            None => None,
            Some(s) => match Regex::new(s) {
                Ok(o) => Some(o),
                Err(e) => {
                    controller.debug("test", id, format!("Unable to compile DaemonReadyText: {}", e));
                    return Some(Err(TestError::DaemonReadyTextError));
                },
            },
        };

        let test_type = match unitfile.get("Test", "Type") {
            None => TestType::Simple,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "simple" => TestType::Simple,
                "daemon" => TestType::Daemon,
                other => return Some(Err(TestError::InvalidType(other.to_string()))),
            },
        };

        let exec_start = match unitfile.get("Test", "ExecStart") {
            None => return Some(Err(TestError::MissingExecSection)),
            Some(s) => s.to_string(),
        };

        let exec_stop_success = match unitfile.get("Test", "ExecStopSuccess") {
            None => match unitfile.get("Test", "ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_failure = match unitfile.get("Test", "ExecStopFail") {
            None => match unitfile.get("Test", "ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let working_directory = match unitfile.get("Test", "WorkingDirectory") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let description = match unitfile.get("Test", "Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match unitfile.get("Test", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let timeout = match unitfile.get("Test", "Timeout") {
            None => config.timeout(),
            Some(s) => match s.parse() {
                Err(_) => return Some(Err(TestError::ParseTimeoutError)),
                Ok(n) => time::Duration::from_secs(n),
            },
        };

        // Get a list of all the requirements, or make a blank list
        let requires = match unitfile.get("Test", "Requires") {
            None => Vec::new(),
            // Split by "," and also whitespace, and combine back into an array.
            Some(s) => s.split(",").map(|x|
                        x.to_string().split_whitespace().map(|y|
                        y.to_string().trim().to_string()).collect()).collect()
        };

        let suggests = match unitfile.get("Test", "Suggests") {
            None => Vec::new(),
            // Split by "," and also whitespace, and combine back into an array.
            Some(s) => s.split(",").map(|x|
                        x.to_string().split_whitespace().map(|y|
                        y.to_string().trim().to_string()).collect()).collect()
        };

        let provides = match unitfile.get("Test", "Provides") {
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
            exec_stop_success_timeout: config.test_success_timeout(),
            exec_stop_failure: exec_stop_failure,
            exec_stop_failure_timeout: config.test_failure_timeout(),
            working_directory: working_directory,
            test_working_directory: Arc::new(Mutex::new(None)),

            controller: controller.clone(),

            last_line: Arc::new(Mutex::new("".to_string())),
            state: Arc::new(Mutex::new(TestState::Pending)),
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

    pub fn timeout(&self) -> time::Duration {
        self.timeout
    }

    /// Start running a test
    ///
    /// Start running a test.  If `working_directory` is specified and
    /// there is no WorkingDirectory in this test, use the provided one.
    pub fn start(&self, scenario_working_directory: &Option<String>, max_duration: time::Duration) {
        self.broadcast(BroadcastMessageContents::Running(self.id().to_string()));

        let test_working_directory = match self.working_directory {
            None => match scenario_working_directory {
                &None => None,
                &Some(ref s) => Some(s.clone()),
            },
            Some(ref s) => Some(s.clone()),
        };

        *(self.test_working_directory.lock().unwrap()) = test_working_directory.clone();

        match self.test_type {
            TestType::Simple => self.start_simple(&test_working_directory, max_duration),
            TestType::Daemon => self.start_daemon(&test_working_directory, max_duration),
        }
    }

    fn start_daemon(&self, working_directory: &Option<String>, max_duration: time::Duration) {

        let result = self.state.clone();
        let id = self.id().to_string();

        // Indicate the daemon is beginning it startup.
        *(self.state.lock().unwrap()) = TestState::Starting;

        // Try to launch the daemon.  If it fails, report the error immediately and return.
        let child = match process::spawn_cmd(self.exec_start.as_str(),
                                             self.id(), self.kind(),
                                             working_directory,
                                             &self.controller) {
            Err(e) => {
                let msg = format!("{:?}", e);
                *(result.lock().unwrap()) = TestState::Fail(msg.clone());
                BroadcastMessageContents::Fail(id, msg);
                return;
            },
            Ok(o) => o,
        };

        // Hook up stderr right away, because we'll be looking for the output on stdout.
        process::log_output(child.stderr, &self.controller.clone(), self.id(), self.kind(), "stderr");

        // Wait until the "match" string appears.
        let mut buf_reader = io::BufReader::new(child.stdout);
        if let Some(ref r) = self.test_daemon_ready {
            // Fire off a thread to kill the process if it takes too long to start.
            let thr_state = self.state.clone();
            let thr_child = child.child.clone();
            let thr_id = self.id().to_string();
            let thr_kind = self.kind().to_string();
            let thr_controller = self.controller.clone();
            let thr_end = self.exec_stop_failure.clone();
            let thr_end_timeout = self.exec_stop_failure_timeout.clone();
            let thr_dir = self.test_working_directory.clone();
            let thr = thread::spawn(move || {
                thread::park_timeout(max_duration);
                if *(thr_state.lock().unwrap()) == TestState::Starting {
                    let msg = format!("Test daemon never came ready");
                    *(thr_state.lock().unwrap()) = TestState::Fail(msg.clone());
                    thr_controller.broadcast(thr_id.as_str(), thr_kind.as_str(), &BroadcastMessageContents::Log(msg));
                    if let Err(e) = thr_child.kill() {
                        thr_controller.broadcast(thr_id.as_str(), thr_kind.as_str(), &BroadcastMessageContents::Log(format!("Unable to kill daemon: {:?}", e)));
                    }

                    if let Some(cmd) = thr_end {
                        thr_controller.broadcast(thr_id.as_str(), thr_kind.as_str(), &BroadcastMessageContents::Log(format!("Running post-test command: {}", cmd)));
                        let ref dir = thr_dir.lock().unwrap();
                        process::try_command(&thr_controller, cmd.as_str(), dir, thr_end_timeout);
                    }
                }
            });

            // Wait for the string to appear.
            self.log(format!("Waiting for string: {}", r));
            loop {
                let mut line = String::new();
                match buf_reader.read_line(&mut line) {
                    Err(e) => {
                        let msg = format!("Error in interface: {:?}", e);
                        self.log(msg.clone());
                        *(self.state.lock().unwrap()) = TestState::Fail(msg.clone());
                        self.broadcast(BroadcastMessageContents::Fail(self.id().to_string(), msg));
                        thr.thread().unpark();
                        self.controller.control_class("result", self.id(), self.kind(), &ControlMessageContents::AdvanceScenario);
                        return;
                    },
                    Ok(0) => {
                        let msg = format!("Test daemon exited");
                        self.log(msg.clone());
                        *(self.state.lock().unwrap()) = TestState::Fail(msg.clone());
                        self.broadcast(BroadcastMessageContents::Fail(self.id().to_string(), msg));
                        thr.thread().unpark();
                        self.controller.control_class("result", self.id(), self.kind(), &ControlMessageContents::AdvanceScenario);
                        return;
                    },
                    Ok(_) => {
                        self.controller.broadcast_class("stdout",
                                     self.id(),
                                     self.kind(),
                                     &BroadcastMessageContents::Log(line.clone()));
                        self.log(format!("Comparing {} to {}", r, line));
                        if r.is_match(line.as_str()) {
                            self.log(format!("It's a match!"));
                            *(self.state.lock().unwrap()) = TestState::Running;
                            break;
                        }
                    },
                }
                line.clear();
            }
            // Now that the match string has been found (if any), mark the daemon as "Running".
            thr.thread().unpark();
        }
        else {
            *(self.state.lock().unwrap()) = TestState::Running;
        }

        process::log_output(buf_reader, &self.controller.clone(), self.id(), self.kind(), "stdout");
        *(self.test_process.lock().unwrap()) = Some(child.child.clone());

        // Move the child into its own thread and wait for it to terminate.
        // If we're still in the "Running" state when it quits, then the daemon
        // has failed.
        let thr_child = child.child.clone();
        let thr_controller = self.controller.clone();
        let thr_id = self.id().to_string();
        let thr_kind = self.kind().to_string();
        let thr_state = self.state.clone();
        thread::spawn(move || {
            let result = thr_child.wait();
            let thr_id2 = thr_id.clone();

            // If we're still in the "Running" state, it's a failure.
            if *(thr_state.lock().unwrap()) == TestState::Running {
                let msg = format!("Daemon exited: {:?}", result);
                *(thr_state.lock().unwrap()) = TestState::Fail(msg.clone());
                thr_controller.broadcast(thr_id2.as_str(), thr_kind.as_str(), &BroadcastMessageContents::Fail(thr_id, msg));
            }
            else {
                thr_controller.broadcast(thr_id2.as_str(), thr_kind.as_str(), &BroadcastMessageContents::Pass(thr_id, "Okay".to_string()));
            }
        });

        // Now that the test is running as a daemon, advance to the next scenario.
        self.controller.control_class("result", self.id(), self.kind(), &ControlMessageContents::AdvanceScenario);
    }

    fn start_simple(&self, working_directory: &Option<String>, max_duration: time::Duration) {
        // Try to create a command.  If this fails, then the command completion will be called,
        // so we can just ignore the error.
        let controller = self.controller.clone();
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        let cmd = self.exec_start.clone();
        let last_line = self.last_line.clone();
        let result = self.state.clone();

        // Mark the test as "Running"
        *(self.state.lock().unwrap()) = TestState::Running;

        let thr_process = self.test_process.clone();
        let child = match process::try_command_completion(
                        cmd.as_str(),
                        working_directory,
                        max_duration,
                        move |res: Result<(), process::CommandError>| {
            let msg = match res {
                Ok(_) => {
                    *(result.lock().unwrap()) = TestState::Pass;
                    BroadcastMessageContents::Pass(id.clone(), last_line.lock().unwrap().to_string())
                },
                Err(e) => {
                    let msg = format!("{:?}", e);
                    *(result.lock().unwrap()) = TestState::Fail(msg.clone());
                    BroadcastMessageContents::Fail(id.clone(), msg)
                },
            };

            // Nullify the current process, since it ought to have exited.
            // If it was an unclean exit this will have already happened.
            *(thr_process.lock().unwrap()) = None;

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
        process::watch_output(child.stdout, &self.controller, self.id(), self.kind(),
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
        process::watch_output(child.stderr, &self.controller, self.id(), self.kind(),
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

        // Save the child process so that we can terminate it early if necessary.
        *(self.test_process.lock().unwrap()) = Some(child.child.clone());
    }

    pub fn stop(&self, working_directory: &Option<String>) {

        // Daemon tests don't respond to stop(), only to terminate().
        if self.test_type == TestType::Daemon {
            return;
        }

        // If the process is still running, make sure it's terminated.
        if let Some(ref pid) = *(self.test_process.lock().unwrap()) {
            if let Err(e) = pid.kill() {
                self.log(format!("Error while killing test: {:?}", e));
            }
        }

        match *(self.state.lock().unwrap()) {
            TestState::Pending | TestState::Starting => (),
            TestState::Running | TestState::Fail(_) => if let Some(ref cmd) = self.exec_stop_failure {
                self.log(format!("Running ExecStopFailure: {}", cmd));
                process::try_command(&self.controller, cmd, working_directory, self.exec_stop_failure_timeout);
            },
            TestState::Pass => if let Some(ref cmd) = self.exec_stop_success {
                self.log(format!("Running ExecStopSuccess: {}", cmd));
                process::try_command(&self.controller, cmd, working_directory, self.exec_stop_success_timeout);
            }
        }
    }

    /// If this is a daemon, stop it.
    pub fn terminate(&self) {
        match self.test_type {
            TestType::Simple => (),
            TestType::Daemon => {
                // If the daemon is still running, then good!  It passed.
                let (cmd, timeout) = if *(self.state.lock().unwrap()) == TestState::Running {
                    *(self.state.lock().unwrap()) = TestState::Pass;
                    (self.exec_stop_success.clone(), self.exec_stop_success_timeout)
                }
                else {
                    (self.exec_stop_failure.clone(), self.exec_stop_failure_timeout)
                };

                // Terminate the process, if it exists.
                if let Some(ref p) = *(self.test_process.lock().unwrap()) {
                    if let Err(e) = p.kill() {
                        self.log(format!("Error while killing daemon: {:?}", e));
                    }
                }
                *(self.test_process.lock().unwrap()) = None;

                if let Some(c) = cmd {
                    self.log(format!("Running post-test command: {}", c));
                    let ref dir = self.test_working_directory.lock().unwrap();
                    process::try_command(&self.controller, c.as_str(), dir, timeout);
                }
            },
        }
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
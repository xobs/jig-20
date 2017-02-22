extern crate ini;
extern crate bus;

use self::ini::Ini;
use std::sync::{Arc, Mutex, mpsc};
use std::collections::HashMap;
use std::time;
use std::fmt;
use std::thread;
use std::io::{BufRead, BufReader};

use cfti::types::Jig;
use cfti::testset::TestSet;
use cfti::controller::{Controller, ControlMessage, BroadcastMessage, BroadcastMessageContents, ControlMessageContents};
use cfti::process;

#[derive(Debug)]
pub enum TestError {
    FileLoadError,
    MissingTestSection,
    MissingExecSection,
    InvalidType(String),
}

#[derive(Debug)]
enum TestType {
    Simple,
    Daemon,
}

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

    /// Timeout: The maximum number of seconds that this test may be run for.
    timeout: u32,

    /// Type: One of "simple" or "daemon".  For "simple" tests, the return code will indicate pass or fail, and each line printed will be considered progress.  For "daemon", the process will be forked and left to run in the background.  See "daemons" below.
    test_type: TestType,

    /// ExecStart: The command to run as part of this test.
    exec_start: String,

    /// ExecStopFail: When stopping tests, if the test failed, then this stop command will be run.
    exec_stop_failure: Option<String>,

    /// ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
    exec_stop_success: Option<String>,

    /// The control channel where control messages go to.
    control: mpsc::Sender<ControlMessage>,

    /// The broadcast bus where broadcast messages come from.
    broadcast: Arc<Mutex<bus::Bus<BroadcastMessage>>>,

    /// The last line outputted by a test, which is the result.
    last_line: Arc<Mutex<String>>,
}

impl fmt::Debug for Test {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[Test]")
    }
}


impl Test {
    pub fn new(ts: &TestSet,
               id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               control: &mpsc::Sender<ControlMessage>,
               broadcast: &Arc<Mutex<bus::Bus<BroadcastMessage>>>) -> Option<Result<Test, TestError>> {

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
                    ts.debug("test", id, format!("The test '{}' is not compatible with this jig", id).as_str());
                    return None;
                }
            }
        }

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
            Some(s) => {
                let vals = s.split(",");
                let mut tmp = Vec::new();
                for val in vals {
                    tmp.push(val.to_string().trim().to_string());
                };
                tmp
            }
        };

        let suggests = match test_section.get("Suggests") {
            None => Vec::new(),
            Some(s) => {
                let vals = s.split(",");
                let mut tmp = Vec::new();
                for val in vals {
                    tmp.push(val.to_string().trim().to_string());
                };
                tmp
            }
        };

        Some(Ok(Test {
            id: id.to_string(),
            name: name,
            description: description,

            requires: requires,
            suggests: suggests,

            test_type: test_type,

            timeout: timeout,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,

            control: control.clone(),
            broadcast: broadcast.clone(),

            last_line: Arc::new(Mutex::new("".to_string())),
        }))
    }

    pub fn describe(&self) {
        Controller::broadcast(&self.broadcast,
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "name".to_string(),
                                                                  self.id().to_string(),
                                                                  self.name().to_string()));
        Controller::broadcast(&self.broadcast,
                              self.id(),
                              self.kind(),
                              &BroadcastMessageContents::Describe(self.kind().to_string(),
                                                                  "description".to_string(),
                                                                  self.id().to_string(),
                                                                  self.description().to_string()));
    }

    /// Start running a test
    ///
    /// Start running a test.  If `working_directory` is specified and
    /// there is no WorkingDirectory in this test, use the provided one.
    pub fn start(&self, working_directory: &Option<String>) {
        self.broadcast(BroadcastMessageContents::Running(self.id().to_string()));

        // Try to create a command.  If this fails, then the command completion will be called,
        // so we can just ignore the error.
        let control = self.control.clone();
        let broadcast = self.broadcast.clone();
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        let cmd = self.exec_start.clone();
        let last_line = self.last_line.clone();
        let (stdout, _) = match process::try_command_completion(
                        cmd.as_str(),
                        working_directory,
                        time::Duration::new(100, 0),
                        move |res: Result<(), process::CommandError>| {
            let msg = match res {
                Ok(_) => BroadcastMessageContents::Pass(id.clone(), last_line.lock().unwrap().to_string()),
                Err(e) => BroadcastMessageContents::Fail(id.clone(), format!("{:?}", e)),
            };

            // Send a message indicating what the test did, and advance the scenario.
            Controller::broadcast_class(&broadcast, "support", id.as_str(), kind.as_str(), &msg);
            Controller::control_class(
                &control,
                "support",
                id.as_str(),
                kind.as_str(),
                &ControlMessageContents::AdvanceScenario);
        }) {
            Err(_) => return,
            Ok(o) => o,
        };

        // Now that the child process is running, hook up the logger.
        let control = self.control.clone();
        let broadcast = self.broadcast.clone();
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        let last_line = self.last_line.clone();
        let builder = thread::Builder::new()
                .name(format!("Running test: {}", id).into());
        builder.spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Err(e) => {
                        println!("Error in interface: {}", e);
                        return;
                    },
                    Ok(l) => {
                        *(last_line.lock().unwrap()) = l.clone();
                        Controller::broadcast(
                            &broadcast,
                            id.as_str(),
                            kind.as_str(),
                            &BroadcastMessageContents::Log(l)
                        );
                    },
                }
            }
        }).unwrap();
    }

    pub fn broadcast(&self, msg: BroadcastMessageContents) {
        Controller::broadcast(&self.broadcast, self.id(), self.kind(), &msg);
    }

    /*
    fn log(&self, msg: &str) {
        self.broadcast(BroadcastMessageContents::Log(msg.to_string()));
    }
    */

    /*
    pub fn control(&self, msg: ControlMessageContents) {
        let controller = self.controller.lock().unwrap();
        controller.send_control_class(
                "support",
                self.id(),
                self.kind(),
                &msg);
    }
    */

    pub fn requirements(&self) -> &Vec<String> {
        &self.requires
    }

    pub fn suggestions(&self) -> &Vec<String> {
        &self.suggests
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
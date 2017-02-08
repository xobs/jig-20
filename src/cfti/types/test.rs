extern crate ini;
use self::ini::Ini;
use std::sync::{Arc, Mutex};
use cfti::types::Jig;
use std::collections::HashMap;
use super::super::testset::TestSet;
use super::super::controller::{self, BroadcastMessageContents};

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

    /// The controller where messages go.
    controller: Arc<Mutex<controller::Controller>>,
}

impl Test {
    pub fn new(ts: &TestSet,
               id: &str,
               path: &str,
               jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               controller: Arc<Mutex<controller::Controller>>) -> Option<Result<Test, TestError>> {

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

            controller: controller,
        }))
    }

    pub fn describe(&self) {
        let controller = self.controller.lock().unwrap();
        controller.send_broadcast(self.id(),
                                self.kind(),
                                BroadcastMessageContents::Describe(self.kind(),
                                                                   "name".to_string(),
                                                                   self.id(),
                                                                   self.name()));
        controller.send_broadcast(self.id(),
                                self.kind(),
                                BroadcastMessageContents::Describe(self.kind(),
                                                                   "description".to_string(),
                                                                   self.id(),
                                                                   self.description()));
    }

    pub fn kind(&self) -> String {
        "test".to_string()
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn description(&self) -> String {
        self.description.clone()
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }
}
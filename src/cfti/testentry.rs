extern crate ini;
use self::ini::Ini;

#[derive(Debug)]
enum TestType {
    Simple,
    Daemon,
}

#[derive(Debug)]
pub struct TestEntry {
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
    exec_stop_fail: String,
    /// ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
    exec_stop_success: String,
}

impl TestEntry {
    pub fn new(path: String) -> Result<TestEntry, &'static str> {
        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err("Unable to load test file"),
            Ok(s) => s,
        };

        let test_section = match ini_file.section(Some("Test")) {
            None => return Err("Test is missing '[Test]' section"),
            Some(s) => s,
        };

        let test_type = match test_section.get("Type") {
            None => TestType::Simple,
            Some(s) => match s.to_string().to_lowercase().as_ref() {
                "simple" => TestType::Simple,
                "daemon" => TestType::Daemon,
                _ => return Err("Test has invalid 'Type'")
            },
        };

        let exec_start = match test_section.get("ExecStart") {
            None => return Err("Test is missing 'ExecStart'"),
            Some(s) => s.to_string(),
        };

        let exec_stop_success = match test_section.get("ExecStopSuccess") {
            None => match test_section.get("ExecStop") {
                    None => "".to_string(),
                    Some(s) => s.to_string(),
                },
            Some(s) => s.to_string(),
        };

        let exec_stop_fail = match test_section.get("ExecStopFail") {
            None => match test_section.get("ExecStop") {
                    None => "".to_string(),
                    Some(s) => s.to_string(),
                },
            Some(s) => s.to_string(),
        };

        let description = match test_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match test_section.get("Name") {
            None => path.clone(),
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

        Ok(TestEntry {
            name: name,
            description: description,

            requires: requires,
            suggests: suggests,

            test_type: test_type,

            timeout: timeout,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_fail: exec_stop_fail,

        })
    }

    pub fn name(&self) -> &String {
        return &self.name;
    }
}
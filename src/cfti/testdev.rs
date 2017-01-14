extern crate ini;
use self::ini::Ini;

use cfti;
use cfti::testentry::TestEntry;
use cfti::testset::TestSet;

#[derive(Debug)]
pub struct TestDev {
    /// The name of the product or device being tested.
    pub name: String,

    /// Description: A longer description of the product or device being tested, up to one paragraph.
    description: String,

    /// Tests: A space- or comma-separated list of tests to be run.  Note that you only need to specify the final test to run, as the dependency graph will fill in the rest.  If you specify multiple tests, then they will be run in the order you specify, possibly with dependency tests added in between.
    tests: Vec<String>,

    /// Success: A command to run if a test plan completes successfully.
    success: String,

    /// Failure: A command to be run if a test plan fails.
    failure: String,
}

impl TestDev {

    pub fn new(path: String) -> Result<TestDev, &'static str> {
        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err("Unable to load test file"),
            Ok(s) => s,
        };

        let plan_section = match ini_file.section(Some("Plan")) {
            None => return Err("Test is missing '[Plan]' section"),
            Some(s) => s,
        };

        let description = match plan_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match plan_section.get("Name") {
            None => path.clone(),
            Some(s) => s.to_string(),
        };

        let success = match plan_section.get("Success") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let failure = match plan_section.get("Failure") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        // Get a list of all the requirements, or make a blank list
        let tests = match plan_section.get("Tests") {
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

        Ok(TestDev {
            name: name,
            description: description,

            success: success,
            failure: failure,

            tests: tests,
        })
    }
}
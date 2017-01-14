use cfti::testentry::TestEntry;
use cfti::testentry::NewTestEntry;
use cfti::testdev::TestDev;
use cfti::testdev::NewTestDev;
use std::collections::HashMap;
use std::fs;

#[derive(Debug)]
enum TestSetEntry {
    TestEntry,
    TestDev,
}

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestDev`.
pub struct TestSet {
    tests: HashMap<String, TestEntry>,
    devs: HashMap<String, TestDev>,
}

pub trait NewTestSet {
    fn new(dir: &str) -> Result<TestSet, &'static str>;
    fn get_dev(&self, target_name: &String) -> Option<&TestDev>;
}

impl NewTestSet for TestSet {

    /// Create a new `TestSet` from the given `dir`
    fn new(dir: &str) -> Result<TestSet, &'static str> {
        let paths = match  fs::read_dir(dir) {
            Ok(dir) => dir,
            Err(_) => return Err("Unable to read directory for some reason")
        };
        let mut tests: HashMap<String, TestEntry> = HashMap::new();
        let mut devs: HashMap<String, TestDev> = HashMap::new();

        for path in paths {
            let pathu = match path {
                Ok(p) => p,
                Err(_) => return Err("Unable to grab path for some reason")
            };
            
            match pathu.file_type() {
                Err(_) => continue,
                Ok(t) => {
                    if !t.is_file(){
                        continue;
                    }
                }
            };

            if pathu.file_name().to_string_lossy().ends_with(".test") {
                let name = String::from(pathu.file_name().to_string_lossy().replace(".test", ""));
                let new_test = TestEntry::new(pathu.path().to_str().unwrap().to_string()).unwrap();
                tests.insert(new_test.name.clone(), new_test);
            }
            else if pathu.file_name().to_string_lossy().ends_with(".dev") {
                let name = String::from(pathu.file_name().to_string_lossy().replace(".plan", ""));

                let new_plan = TestDev::new(pathu.path().to_str().unwrap().to_string()).unwrap();
                devs.insert(new_plan.name.clone(), new_plan);
            }
        }

        let test_set = TestSet {
            tests: tests,
            devs: devs,
        };

        Ok(test_set)
    }

    fn get_dev(&self, dev_name: &String) -> Option<&TestDev> {
        return self.devs.get(dev_name);
    }
}
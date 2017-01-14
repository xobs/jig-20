use cfti::testentry::TestEntry;
use cfti::testtarget::TestTarget;
use std::collections::HashMap;
use std::fs;

/// A `TestSet` object holds every known test in an unordered fashion.
/// To run, a `TestSet` must be converted into a `TestTarget`.
#[derive(Debug)]
pub struct TestSet {
    tests: HashMap<String, TestEntry>,
    devs: HashMap<String, TestTarget>,
}

impl TestSet {
    /// Create a new `TestSet` from the given `dir`
    pub fn new(dir: &str) -> Result<TestSet, &'static str> {
        let paths = match fs::read_dir(dir) {
            Ok(dir) => dir,
            Err(_) => return Err("Unable to read directory for some reason"),
        };
        let mut tests: HashMap<String, TestEntry> = HashMap::new();
        let mut devs: HashMap<String, TestTarget> = HashMap::new();

        for path in paths {
            let pathu = match path {
                Ok(p) => p,
                Err(_) => return Err("Unable to grab path for some reason"),
            };

            match pathu.file_type() {
                Err(_) => continue,
                Ok(t) => {
                    if !t.is_file() {
                        continue;
                    }
                }
            };

            if pathu.file_name().to_string_lossy().ends_with(".test") {
                let new_test = TestEntry::new(pathu.path().to_str().unwrap().to_string()).unwrap();
                tests.insert(new_test.name().clone(), new_test);
            } else if pathu.file_name().to_string_lossy().ends_with(".target") {
                let new_plan = TestTarget::new(pathu.path().to_str().unwrap().to_string()).unwrap();
                devs.insert(new_plan.name.clone(), new_plan);
            }
        }

        let test_set = TestSet {
            tests: tests,
            devs: devs,
        };

        Ok(test_set)
    }

    pub fn get_dev(&self, dev_name: &String) -> Option<&TestTarget> {
        return self.devs.get(dev_name);
    }

    pub fn all_tests(&self) -> Vec<&TestEntry> {
        let mut sorted_keys: Vec<&String> = self.tests.keys().collect();
        sorted_keys.sort();

        let mut result: Vec<&TestEntry> = Vec::new();
        for key in sorted_keys {
            result.push(self.tests.get(key).unwrap());
        }
        result
    }
}
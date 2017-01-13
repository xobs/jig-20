extern crate ini;
use self::ini::Ini;
use std::fs;
use std::collections::HashMap;

#[derive(Debug)]
pub struct TestEntry {
    name: String,
    exec_start: String,
    exec_stop: String,
    short_name: String,
    description: String,
    timeout: u32,
    requires: Vec<String>,
    arguments: Vec<String>,
}

#[derive(Debug)]
pub struct TestSet {
    tests: HashMap<String, TestEntry>,
}

#[derive(Debug)]
pub struct TestPlan<'a> {
    tests: Vec<&'a TestEntry>,
}
pub trait FindTests {
    fn ordered_tests(&self, ending_test: &str) -> Result<TestPlan, &'static str>;
}

pub fn read_dir(dir: &str) -> Result<TestSet, &'static str> {
    let paths = match  fs::read_dir(dir) {
        Ok(dir) => dir,
        Err(_) => return Err("Unable to read directory for some reason")
    };
    let mut tests: HashMap<String, TestEntry> = HashMap::new();

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

        if !pathu.file_name().to_string_lossy().ends_with(".test") {
            continue;
        }

        let name = String::from(pathu.file_name().to_string_lossy().replace(".test", ""));

        // Load the .ini file
        let ini_file = match Ini::load_from_file(pathu.path()) {
            Err(_) => return Err("Unable to load test file"),
            Ok(s) => s,
        };

        let test_section = match ini_file.section(Some("Test")) {
            None => return Err("Test is missing '[Test]' section"),
            Some(s) => s,
        };

        let exec_start = match test_section.get("ExecStart") {
            None => return Err("Test is missing 'ExecStart'"),
            Some(s) => s.to_string(),
        };

        let exec_stop = match test_section.get("ExecStop") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let description = match test_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let short_name = match test_section.get("Name") {
            None => name.clone(),
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

        let args: Vec<String> = match ini_file.section(Some("Exec")) {
            None => Vec::new(),
            Some(s) => {
                let mut args = Vec::new();
                for arg in s.keys()/*.sort()*/ {
                    //println!("Key: {}", s.get(arg).unwrap());
                    args.push(s.get(arg).unwrap().to_string());
                }
                args
            },
        };

        let new_test = TestEntry {
            name: name,
            exec_start: exec_start,

            short_name: short_name,
            timeout: timeout,
            exec_stop: exec_stop,
            description: description,

            requires: requires,
            arguments: args,
        };
        tests.insert(new_test.name.clone(), new_test);
    }

    let test_set = TestSet {
        tests: tests,
    };

    Ok(test_set)
}

impl FindTests for TestSet {
    fn ordered_tests(&self, ending_test_name: &str) -> Result<TestPlan, &'static str> {
        let ending_test = match self.tests.get(ending_test_name) {
            None => return Err("Couldn't find final test"),
            Some(t) => t,
        };

        let mut test_list = Vec::new();
        test_list.push(ending_test);
        let test_set = TestPlan {
            tests: test_list,
        };
        Ok(test_set)
    }
}
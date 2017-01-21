extern crate ini;
use self::ini::Ini;
use std::collections::HashMap;
use super::test::Test;

#[derive(Debug)]
pub struct Scenario {
    id: String,

    tests: HashMap<String, Test>,

    success: Option<String>,
    failure: Option<String>,
}

impl Scenario {
    pub fn new(id: &str, path: &str) -> Result<Scenario, &'static str> {

        Ok(Scenario {
            id: id.to_string(),
            tests: HashMap::new(),
            success: None,
            failure: None,
        })
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}
mod cfti;
use cfti::types::Test;
use cfti::types::Scenario;
use cfti::TestSet;
use cfti::controller;
use std::{thread, time};
use std::sync::{Arc, Mutex};

fn main() {

    let controller = Arc::new(Mutex::new(cfti::controller::Controller::new().unwrap()));
    let test_set = cfti::TestSet::new("ltc-tests", controller.clone()).unwrap();

    for i in 0..10 {
        println!("i: {}", i);
        test_set.debug("main", "main", format!("I loop: {}", i).as_str());
        thread::sleep(time::Duration::from_millis(100));
    }
    println!("Test set: {:?}", test_set);
}
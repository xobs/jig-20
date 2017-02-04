mod cfti;
use std::{thread, time};
use std::sync::{Arc, Mutex};

fn main() {

    let controller = cfti::controller::Controller::new().unwrap();
    let test_set = cfti::TestSet::new("ltc-tests", controller.clone()).unwrap();

/*
    for i in 0..10 {
        println!("i: {}", i);
        test_set.lock().unwrap().debug("main", "main", format!("I loop: {}", i).as_str());
        thread::sleep(time::Duration::from_millis(100));
    }
*/

    println!("Test set: {:?}", test_set);
    loop {
        thread::sleep(time::Duration::from_millis(100));
    }
}
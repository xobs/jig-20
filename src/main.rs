mod cfti;
use cfti::types::Test;
use cfti::types::Scenario;
use cfti::TestSet;
use std::{thread, time};

fn main() {

    let test_set = cfti::TestSet::new("ltc-tests").unwrap();

    for i in 0..10 {
        println!("i: {}", i);
        test_set.debug("main", "main", format!("I loop: {}", i).as_str());
        thread::sleep(time::Duration::from_millis(100));
    }
    println!("Test set: {:?}", test_set);
}
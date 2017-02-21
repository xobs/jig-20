mod cfti;
use std::{thread, time};

fn main() {
    let controller = cfti::controller::Controller::new().unwrap();
    let test_set = cfti::TestSet::new("ltc-tests", controller.clone()).unwrap();

    println!("Test set: {:?}", test_set);
    loop {
        match controller.try_lock() {
            Ok(lock) => {
                if lock.should_exit() {
                    break;
                }
            },
            Err(e) => println!("Controller mutex is locked: {:?}", e),
        };
        thread::sleep(time::Duration::from_millis(100));
    }
}

mod cfti;
use std::{thread, time};

fn main() {
    let mut controller = cfti::controller::Controller::new().unwrap();
    let test_set = cfti::TestSet::new("ltc-tests", &mut controller).unwrap();

    println!("Test set: {:?}", test_set);
    loop {
        if controller.should_exit() {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }
}

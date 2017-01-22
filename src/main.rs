mod cfti;
use cfti::types::Test;
use cfti::types::Scenario;
use cfti::TestSet;

fn main() {

    let test_set = cfti::TestSet::new("ltc-tests").unwrap();
    println!("Test set: {:?}", test_set);
}
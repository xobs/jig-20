mod cfti;
use cfti::testset;
use cfti::testplan;

fn main() {
    println!("Hello, world!");
    let test_set = cfti::testset::TestSet::new("ltc-tests").unwrap();
    println!("Test set: {:?}", test_set);
    let plan = test_set.get_dev(&"Program App".to_string()).unwrap();
    println!("Tests: {:?}", plan);
}
mod cfti;
use cfti::testset;
use cfti::testset::NewTestSet;

fn main() {
    println!("Hello, world!");
    let testset = cfti::testset::TestSet::new("ltc-tests").unwrap();
    let plan = testset.get_dev(&"program-app".to_string()).unwrap();
    println!("Tests: {:?}", plan);
}
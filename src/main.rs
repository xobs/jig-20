
mod cfti;
use cfti::testentry::FindTests;

fn main() {
    println!("Hello, world!");
    let tests = cfti::testentry::read_dir("ltc-tests").unwrap();
    let plan = tests.ordered_tests("program-app").unwrap();
    println!("Tests: {:?}", plan);
}
